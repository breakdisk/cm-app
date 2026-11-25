//! The base64 document upload, shared by the courier's own route
//! (`/me/documents/upload`) and the admin's on-behalf route
//! (`/admin/profiles/:profile_id/documents`).
//!
//! One body type and one validation path: the two routes differ in *who* is
//! acting and *whose* profile it lands on, never in what a valid upload is.

use base64::Engine as _;
use chrono::NaiveDate;
use serde::Deserialize;
use uuid::Uuid;
use logisticos_errors::AppError;
use crate::api::http::AppState;

#[derive(Deserialize)]
pub struct UploadDocumentRequest {
    #[serde(default)]
    pub document_type_id:   Option<Uuid>,
    #[serde(default)]
    pub document_type_code: Option<String>,
    pub document_number:    String,
    pub file_base64:        String,
    pub content_type:       String,
    pub issue_date:         Option<String>,
    pub expiry_date:        Option<String>,
}

/// An upload that has passed validation, before any I/O beyond the type lookup.
pub struct ParsedUpload {
    pub document_number: String,
    pub bytes:           Vec<u8>,
    pub issue_date:      Option<NaiveDate>,
    pub expiry_date:     Option<NaiveDate>,
}

/// An upload that is validated and in storage, ready for `submit_document`.
pub struct StoredUpload {
    pub document_type_id: Uuid,
    pub document_number:  String,
    pub issue_date:       Option<NaiveDate>,
    pub expiry_date:      Option<NaiveDate>,
    pub file_url:         String,
}

pub async fn resolve_document_type_id(
    state: &AppState,
    type_id: Option<Uuid>,
    type_code: Option<&str>,
) -> Result<Uuid, AppError> {
    match type_id {
        Some(id) => Ok(id),
        None => {
            let code = type_code.ok_or_else(|| AppError::Validation(
                "document_type_id or document_type_code is required".into(),
            ))?;
            Ok(state.compliance.doc_types
                .find_by_code(code)
                .await?
                .ok_or_else(|| AppError::NotFound {
                    resource: "DocumentType",
                    id: code.to_string(),
                })?
                .id)
        }
    }
}

pub fn parse_date(s: Option<String>) -> Result<Option<NaiveDate>, AppError> {
    match s {
        None => Ok(None),
        Some(d) => NaiveDate::parse_from_str(&d, "%Y-%m-%d")
            .map(Some)
            .map_err(|_| AppError::Validation(
                format!("Invalid date format '{}'; expected YYYY-MM-DD", d),
            )),
    }
}

/// Everything about the body that can be checked without a database or storage.
///
/// Also runs before the upload, so a bad date can no longer leave an orphaned
/// object in the bucket behind a rejected request.
pub fn parse_upload(req: &UploadDocumentRequest) -> Result<ParsedUpload, AppError> {
    let document_number = req.document_number.trim().to_owned();
    if document_number.is_empty() {
        return Err(AppError::Validation("document_number cannot be empty".into()));
    }
    if document_number.len() > 100 {
        return Err(AppError::Validation("document_number must be 100 characters or fewer".into()));
    }

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(req.file_base64.as_bytes())
        .map_err(|e| AppError::Validation(format!("Invalid base64 payload: {e}")))?;

    Ok(ParsedUpload {
        document_number,
        bytes,
        issue_date:  parse_date(req.issue_date.clone())?,
        expiry_date: parse_date(req.expiry_date.clone())?,
    })
}

/// Resolve the type, validate the body and put the file in tenant storage.
pub async fn store_upload(
    state: &AppState,
    tenant_id: Uuid,
    req: UploadDocumentRequest,
) -> Result<StoredUpload, AppError> {
    let document_type_id = resolve_document_type_id(
        state,
        req.document_type_id,
        req.document_type_code.as_deref(),
    ).await?;
    let parsed = parse_upload(&req)?;

    let file_url = state.storage
        .upload(tenant_id, parsed.bytes, &req.content_type)
        .await
        .map_err(|e| {
            tracing::error!(error = ?e, "compliance document S3 upload failed");
            AppError::Validation("Document upload failed; please try again".into())
        })?;

    Ok(StoredUpload {
        document_type_id,
        document_number: parsed.document_number,
        issue_date:      parsed.issue_date,
        expiry_date:     parsed.expiry_date,
        file_url,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req() -> UploadDocumentRequest {
        UploadDocumentRequest {
            document_type_id:   None,
            document_type_code: Some("DRIVERS_LICENSE".into()),
            document_number:    " N01-23 ".into(),
            file_base64:        "aGVsbG8=".into(),
            content_type:       "image/jpeg".into(),
            issue_date:         Some("2024-01-31".into()),
            expiry_date:        None,
        }
    }

    #[test]
    fn a_valid_body_parses_and_trims_the_number() {
        let p = parse_upload(&req()).expect("valid");
        assert_eq!(p.document_number, "N01-23");
        assert_eq!(p.bytes, b"hello");
        assert_eq!(p.issue_date, NaiveDate::from_ymd_opt(2024, 1, 31));
        assert_eq!(p.expiry_date, None);
    }

    #[test]
    fn a_blank_or_oversized_number_is_a_validation_error() {
        let mut r = req();
        r.document_number = "   ".into();
        assert!(matches!(parse_upload(&r), Err(AppError::Validation(_))));
        r.document_number = "x".repeat(101);
        assert!(matches!(parse_upload(&r), Err(AppError::Validation(_))));
    }

    #[test]
    fn bad_base64_is_a_validation_error() {
        let mut r = req();
        r.file_base64 = "not base64!!".into();
        assert!(matches!(parse_upload(&r), Err(AppError::Validation(_))));
    }

    #[test]
    fn a_date_not_in_iso_form_is_a_validation_error() {
        let mut r = req();
        r.expiry_date = Some("31/01/2030".into());
        assert!(matches!(parse_upload(&r), Err(AppError::Validation(_))));
    }
}
