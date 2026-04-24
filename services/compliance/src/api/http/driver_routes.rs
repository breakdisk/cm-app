use axum::{extract::{Path, State}, Json};
use std::sync::Arc;
use uuid::Uuid;
use logisticos_auth::middleware::AuthClaims;
use logisticos_errors::AppError;
use serde::Deserialize;
use crate::api::http::AppState;

pub async fn get_my_profile(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let profile = state.compliance.profiles
        .find_by_entity(claims.tenant_id, "driver", claims.user_id)
        .await?
        .ok_or(AppError::NotFound { resource: "ComplianceProfile", id: claims.user_id.to_string() })?;

    let required = state.compliance.doc_types
        .list_required_for("driver", &profile.jurisdiction)
        .await?;
    let docs = state.compliance.documents
        .list_by_profile(profile.id)
        .await?;

    Ok(Json(serde_json::json!({
        "data": { "profile": profile, "required_types": required, "documents": docs }
    })))
}

#[derive(Deserialize)]
pub struct SubmitDocumentRequest {
    pub document_type_id: Uuid,
    pub document_number:  String,
    pub issue_date:       Option<String>,
    pub expiry_date:      Option<String>,
    pub file_url:         String,
}

pub async fn submit_document(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
    Json(req): Json<SubmitDocumentRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let profile = state.compliance.profiles
        .find_by_entity(claims.tenant_id, "driver", claims.user_id)
        .await?
        .ok_or(AppError::NotFound { resource: "ComplianceProfile", id: claims.user_id.to_string() })?;

    // Validate file_url: must be a non-empty https:// or http:// URL.
    let file_url = req.file_url.trim().to_owned();
    if file_url.is_empty() || (!file_url.starts_with("https://") && !file_url.starts_with("http://")) {
        return Err(AppError::Validation(
            "file_url must be a valid http:// or https:// URL".into(),
        ));
    }

    // Validate document_number: 1–100 characters, no leading/trailing whitespace.
    let document_number = req.document_number.trim().to_owned();
    if document_number.is_empty() {
        return Err(AppError::Validation("document_number cannot be empty".into()));
    }
    if document_number.len() > 100 {
        return Err(AppError::Validation(
            "document_number must be 100 characters or fewer".into(),
        ));
    }

    let parse_date = |s: Option<String>| -> Result<Option<chrono::NaiveDate>, AppError> {
        match s {
            None => Ok(None),
            Some(d) => chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d")
                .map(Some)
                .map_err(|_| AppError::Validation(format!("Invalid date format '{}'; expected YYYY-MM-DD", d))),
        }
    };

    let doc = state.compliance.submit_document(
        profile.id,
        req.document_type_id,
        document_number,
        parse_date(req.issue_date)?,
        parse_date(req.expiry_date)?,
        file_url,
        claims.user_id,
    ).await?;

    Ok(Json(serde_json::json!({ "data": doc })))
}

pub async fn get_document(
    AuthClaims(claims): AuthClaims,
    Path(doc_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let doc = state.compliance.documents.find_by_id(doc_id).await?
        .ok_or(AppError::NotFound { resource: "DriverDocument", id: doc_id.to_string() })?;
    let profile = state.compliance.profiles.find_by_id(doc.compliance_profile_id).await?
        .ok_or(AppError::NotFound { resource: "ComplianceProfile", id: doc.compliance_profile_id.to_string() })?;
    if profile.entity_id != claims.user_id {
        return Err(AppError::Forbidden { resource: "DriverDocument".to_owned() });
    }
    Ok(Json(serde_json::json!({ "data": doc })))
}

/// POST /me/documents/upload — base64 upload + submit in one call.
///
/// Client encodes the image/PDF to base64 and POSTs with metadata. Server
/// decodes, uploads to S3, then creates the DriverDocument record referencing
/// the resulting s3:// URI. The pre-existing `submit_document` handler still
/// accepts a pre-uploaded file_url for flows where the client already has an
/// S3 URI (e.g. admin-side bulk ingest).
#[derive(Deserialize)]
pub struct UploadDocumentRequest {
    /// Accept either `document_type_id` (UUID) or `document_type_code`
    /// (e.g. "passport"); at least one is required. Code is friendlier for
    /// mobile clients that hardcode a known list (passport / emirates_id /
    /// drivers_license) without looking up UUIDs first.
    #[serde(default)]
    pub document_type_id:   Option<Uuid>,
    #[serde(default)]
    pub document_type_code: Option<String>,
    pub document_number:    String,
    /// Base64-encoded file bytes (image/jpeg, image/png, or application/pdf).
    pub file_base64:        String,
    /// MIME type — validated server-side against the storage allow-list.
    pub content_type:       String,
    pub issue_date:         Option<String>,
    pub expiry_date:        Option<String>,
}

pub async fn upload_document(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
    Json(req): Json<UploadDocumentRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    use base64::Engine as _;

    let profile = state.compliance.profiles
        .find_by_entity(claims.tenant_id, "driver", claims.user_id)
        .await?
        .ok_or(AppError::NotFound { resource: "ComplianceProfile", id: claims.user_id.to_string() })?;

    // Resolve document_type_id from either the UUID or the code lookup.
    let document_type_id = match req.document_type_id {
        Some(id) => id,
        None => {
            let code = req.document_type_code.as_deref().ok_or_else(|| AppError::Validation(
                "document_type_id or document_type_code is required".into(),
            ))?;
            state.compliance.doc_types
                .find_by_code(code)
                .await?
                .ok_or_else(|| AppError::NotFound {
                    resource: "DocumentType",
                    id: code.to_string(),
                })?
                .id
        }
    };

    let document_number = req.document_number.trim().to_owned();
    if document_number.is_empty() {
        return Err(AppError::Validation("document_number cannot be empty".into()));
    }
    if document_number.len() > 100 {
        return Err(AppError::Validation("document_number must be 100 characters or fewer".into()));
    }

    // Decode the base64 payload. Storage.upload() enforces size + content type.
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(req.file_base64.as_bytes())
        .map_err(|e| AppError::Validation(format!("Invalid base64 payload: {e}")))?;

    let file_url = state.storage
        .upload(claims.tenant_id, bytes, &req.content_type)
        .await
        .map_err(|e| AppError::Validation(e.to_string()))?;

    let parse_date = |s: Option<String>| -> Result<Option<chrono::NaiveDate>, AppError> {
        match s {
            None => Ok(None),
            Some(d) => chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d")
                .map(Some)
                .map_err(|_| AppError::Validation(format!("Invalid date format '{}'; expected YYYY-MM-DD", d))),
        }
    };

    let doc = state.compliance.submit_document(
        profile.id,
        document_type_id,
        document_number,
        parse_date(req.issue_date)?,
        parse_date(req.expiry_date)?,
        file_url,
        claims.user_id,
    ).await?;

    Ok(Json(serde_json::json!({ "data": doc })))
}

/// GET /me/documents/:doc_id/url — returns a 15-minute presigned download URL
pub async fn get_document_url(
    AuthClaims(claims): AuthClaims,
    Path(doc_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let doc = state.compliance.documents.find_by_id(doc_id).await?
        .ok_or(AppError::NotFound { resource: "DriverDocument", id: doc_id.to_string() })?;
    let profile = state.compliance.profiles.find_by_id(doc.compliance_profile_id).await?
        .ok_or(AppError::NotFound { resource: "ComplianceProfile", id: doc.compliance_profile_id.to_string() })?;
    if profile.entity_id != claims.user_id {
        return Err(AppError::Forbidden { resource: "DriverDocument".to_owned() });
    }
    let url = state.storage.presign_url(&doc.file_url).await?;
    Ok(Json(serde_json::json!({ "data": { "url": url, "expires_in": 900 } })))
}
