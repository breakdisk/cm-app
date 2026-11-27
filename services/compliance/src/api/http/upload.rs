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
use crate::api::http::admin_routes::sniff_content_type;
use crate::domain::entities::{ComplianceProfile, DocumentType};
use crate::domain::repositories::DocumentTypeRepository;

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

/// The document type a request names, by id or by code.
///
/// An id is looked up rather than passed through: an unknown one used to reach
/// `submit_document` and fail on the foreign key as a 500, after the file was
/// already in storage.
pub async fn resolve_document_type(
    doc_types: &dyn DocumentTypeRepository,
    type_id:   Option<Uuid>,
    type_code: Option<&str>,
) -> Result<DocumentType, AppError> {
    match type_id {
        Some(id) => doc_types.find_by_id(id).await?
            .ok_or_else(|| AppError::NotFound { resource: "DocumentType", id: id.to_string() }),
        None => {
            let code = type_code.ok_or_else(|| AppError::Validation(
                "document_type_id or document_type_code is required".into(),
            ))?;
            doc_types.find_by_code(code).await?
                .ok_or_else(|| AppError::NotFound { resource: "DocumentType", id: code.to_string() })
        }
    }
}

pub async fn resolve_document_type_id(
    state: &AppState,
    type_id: Option<Uuid>,
    type_code: Option<&str>,
) -> Result<Uuid, AppError> {
    Ok(resolve_document_type(state.compliance.doc_types.as_ref(), type_id, type_code).await?.id)
}

/// Does this document type apply to this profile?
///
/// Status recompute counts documents against how many types are required and
/// never looks at *which* types they are, so an approved document of a type the
/// profile is not asked for would make a courier Compliant. The gate has to be
/// here, before the file is stored.
pub fn ensure_type_fits_profile(
    doc_type: &DocumentType,
    profile:  &ComplianceProfile,
) -> Result<(), AppError> {
    if !doc_type.applicable_to.iter().any(|t| t == &profile.entity_type) {
        return Err(AppError::Validation(format!(
            "Document type '{}' does not apply to a {} profile.",
            doc_type.code, profile.entity_type,
        )));
    }
    if !doc_type.jurisdiction.eq_ignore_ascii_case(&profile.jurisdiction) {
        return Err(AppError::Validation(format!(
            "Document type '{}' is for jurisdiction {}, but this profile is in {}.",
            doc_type.code, doc_type.jurisdiction, profile.jurisdiction,
        )));
    }
    Ok(())
}

/// Resolve the named type and prove it fits the profile, in one step so a
/// caller cannot do the first and forget the second. Nothing here touches
/// storage; the admin handler calls this before `store_upload_of_type`.
pub async fn resolve_type_for_profile(
    doc_types: &dyn DocumentTypeRepository,
    profile:   &ComplianceProfile,
    type_id:   Option<Uuid>,
    type_code: Option<&str>,
) -> Result<DocumentType, AppError> {
    let doc_type = resolve_document_type(doc_types, type_id, type_code).await?;
    ensure_type_fits_profile(&doc_type, profile)?;
    Ok(doc_type)
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

fn normalize_content_type(declared: &str) -> String {
    let base = declared.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    if base == "image/jpg" { "image/jpeg".into() } else { base }
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

    // The declared type is what the object is stored and served as, so it has
    // to be what the bytes are.
    let declared = normalize_content_type(&req.content_type);
    let actual = sniff_content_type(&bytes);
    if declared != actual {
        return Err(AppError::Validation(format!(
            "content_type '{}' does not match the file, which looks like {actual}",
            req.content_type,
        )));
    }

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
    store_upload_of_type(state, tenant_id, document_type_id, req).await
}

/// Validate the body and store it, for a type the caller has already resolved
/// and checked.
pub async fn store_upload_of_type(
    state: &AppState,
    tenant_id: Uuid,
    document_type_id: Uuid,
    req: UploadDocumentRequest,
) -> Result<StoredUpload, AppError> {
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

    /// `FF D8 FF E0`: enough of a JPEG for the sniffer.
    const JPEG_B64: &str = "/9j/4A==";

    fn req() -> UploadDocumentRequest {
        UploadDocumentRequest {
            document_type_id:   None,
            document_type_code: Some("DRIVERS_LICENSE".into()),
            document_number:    " N01-23 ".into(),
            file_base64:        JPEG_B64.into(),
            content_type:       "image/jpeg".into(),
            issue_date:         Some("2024-01-31".into()),
            expiry_date:        None,
        }
    }

    #[test]
    fn a_valid_body_parses_and_trims_the_number() {
        let p = parse_upload(&req()).expect("valid");
        assert_eq!(p.document_number, "N01-23");
        assert_eq!(p.bytes, vec![0xFF, 0xD8, 0xFF, 0xE0]);
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

    #[test]
    fn a_declared_type_that_disagrees_with_the_bytes_is_refused() {
        let mut r = req();
        r.content_type = "application/pdf".into();
        assert!(matches!(parse_upload(&r), Err(AppError::Validation(_))));
        // Bytes that are none of the four accepted formats never match.
        r.content_type = "image/jpeg".into();
        r.file_base64 = "aGVsbG8=".into();
        assert!(matches!(parse_upload(&r), Err(AppError::Validation(_))));
    }

    #[test]
    fn declared_type_case_params_and_jpg_alias_are_tolerated() {
        let mut r = req();
        for ct in ["IMAGE/JPEG", "image/jpeg; charset=binary", "image/jpg"] {
            r.content_type = ct.into();
            assert!(parse_upload(&r).is_ok(), "{ct}");
        }
    }

    // --- document type vs profile -------------------------------------

    use std::sync::Mutex;
    use async_trait::async_trait;

    fn dtype(code: &str, jurisdiction: &str, applicable_to: &[&str]) -> DocumentType {
        DocumentType {
            id: Uuid::new_v4(), code: code.into(), jurisdiction: jurisdiction.into(),
            applicable_to: applicable_to.iter().map(|s| (*s).to_owned()).collect(),
            name: code.into(), description: None, is_required: true, has_expiry: true,
            warn_days_before: 30, grace_period_days: 7, vehicle_classes: None,
        }
    }

    fn driver_profile(jurisdiction: &str) -> ComplianceProfile {
        use crate::domain::entities::ComplianceStatus;
        ComplianceProfile {
            id: Uuid::new_v4(), tenant_id: Uuid::new_v4(), entity_type: "driver".into(),
            entity_id: Uuid::new_v4(), overall_status: ComplianceStatus::PendingSubmission,
            jurisdiction: jurisdiction.into(), last_reviewed_at: None, reviewed_by: None,
            suspended_at: None, created_at: chrono::Utc::now(), updated_at: chrono::Utc::now(),
        }
    }

    struct Types(Mutex<Vec<DocumentType>>);

    #[async_trait]
    impl DocumentTypeRepository for Types {
        async fn find_by_code(&self, code: &str) -> anyhow::Result<Option<DocumentType>> {
            Ok(self.0.lock().unwrap().iter().find(|t| t.code == code).cloned())
        }
        async fn list_required_for(&self, _: &str, _: &str) -> anyhow::Result<Vec<DocumentType>> {
            Ok(vec![])
        }
        async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<DocumentType>> {
            Ok(self.0.lock().unwrap().iter().find(|t| t.id == id).cloned())
        }
        async fn list_all(&self) -> anyhow::Result<Vec<DocumentType>> {
            Ok(self.0.lock().unwrap().clone())
        }
    }

    // These three return before the handler reaches `store_upload_of_type`:
    // the handler's only route to storage is after this call's `?`.

    #[tokio::test]
    async fn a_type_from_another_jurisdiction_is_refused() {
        let t = dtype("MY_LICENCE", "MY", &["driver"]);
        let repo = Types(Mutex::new(vec![t.clone()]));
        let err = resolve_type_for_profile(&repo, &driver_profile("PH"), Some(t.id), None)
            .await.unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[tokio::test]
    async fn a_type_for_another_entity_type_is_refused() {
        let t = dtype("KYC_ID", "PH", &["customer"]);
        let repo = Types(Mutex::new(vec![t.clone()]));
        let err = resolve_type_for_profile(&repo, &driver_profile("PH"), None, Some("KYC_ID"))
            .await.unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[tokio::test]
    async fn an_unknown_type_id_is_404() {
        let repo = Types(Mutex::new(vec![]));
        let err = resolve_type_for_profile(&repo, &driver_profile("PH"), Some(Uuid::new_v4()), None)
            .await.unwrap_err();
        assert!(matches!(err, AppError::NotFound { .. }));
        assert_eq!(err.status_code(), axum::http::StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn a_matching_type_is_accepted() {
        let t = dtype("DRIVERS_LICENSE", "PH", &["driver", "customer"]);
        let repo = Types(Mutex::new(vec![t.clone()]));
        let got = resolve_type_for_profile(&repo, &driver_profile("ph"), Some(t.id), None)
            .await.expect("fits");
        assert_eq!(got.id, t.id);
    }
}
