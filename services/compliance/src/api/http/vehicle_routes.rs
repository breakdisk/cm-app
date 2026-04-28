use axum::{extract::{Path, State}, Json};
use std::sync::Arc;
use uuid::Uuid;
use logisticos_auth::middleware::AuthClaims;
use logisticos_auth::rbac::permissions;
use logisticos_errors::AppError;
use serde::Deserialize;
use crate::api::http::AppState;

/// GET /api/v1/compliance/vehicles/:vehicle_id/profile
/// Returns the vehicle's compliance profile + required document types + submitted documents.
/// Requires FLEET_READ or COMPLIANCE_REVIEW.
pub async fn get_vehicle_profile(
    AuthClaims(claims): AuthClaims,
    Path(vehicle_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    claims.require_any_permission(&[permissions::FLEET_READ, permissions::COMPLIANCE_REVIEW])?;

    let profile = state.compliance.profiles
        .find_by_entity(claims.tenant_id, "vehicle", vehicle_id)
        .await?
        .ok_or(AppError::NotFound { resource: "VehicleComplianceProfile", id: vehicle_id.to_string() })?;

    let required = state.compliance.doc_types
        .list_required_for("vehicle", &profile.jurisdiction)
        .await?;
    let docs = state.compliance.documents
        .list_by_profile(profile.id)
        .await?;

    Ok(Json(serde_json::json!({
        "data": { "profile": profile, "required_types": required, "documents": docs }
    })))
}

#[derive(Deserialize)]
pub struct SubmitVehicleDocumentRequest {
    pub document_type_id: Uuid,
    pub document_number:  String,
    pub issue_date:       Option<String>,
    pub expiry_date:      Option<String>,
    pub file_url:         String,
}

/// POST /api/v1/compliance/vehicles/:vehicle_id/documents
/// Submits a compliance document for a vehicle. Requires FLEET_MANAGE or COMPLIANCE_ADMIN.
pub async fn submit_vehicle_document(
    AuthClaims(claims): AuthClaims,
    Path(vehicle_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
    Json(req): Json<SubmitVehicleDocumentRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    claims.require_any_permission(&[permissions::FLEET_MANAGE, permissions::COMPLIANCE_ADMIN])?;

    let profile = state.compliance.profiles
        .find_by_entity(claims.tenant_id, "vehicle", vehicle_id)
        .await?
        .ok_or(AppError::NotFound { resource: "VehicleComplianceProfile", id: vehicle_id.to_string() })?;

    let file_url = req.file_url.trim().to_owned();
    if file_url.is_empty() || (!file_url.starts_with("https://") && !file_url.starts_with("http://")) {
        return Err(AppError::Validation("file_url must be a valid http:// or https:// URL".into()));
    }

    let document_number = req.document_number.trim().to_owned();
    if document_number.is_empty() {
        return Err(AppError::Validation("document_number cannot be empty".into()));
    }
    if document_number.len() > 100 {
        return Err(AppError::Validation("document_number must be 100 characters or fewer".into()));
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

#[derive(Deserialize)]
pub struct UploadVehicleDocumentRequest {
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

/// POST /api/v1/compliance/vehicles/:vehicle_id/documents/upload
/// Base64 upload + submit in one call. Requires FLEET_MANAGE or COMPLIANCE_ADMIN.
pub async fn upload_vehicle_document(
    AuthClaims(claims): AuthClaims,
    Path(vehicle_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
    Json(req): Json<UploadVehicleDocumentRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    use base64::Engine as _;

    claims.require_any_permission(&[permissions::FLEET_MANAGE, permissions::COMPLIANCE_ADMIN])?;

    let profile = state.compliance.profiles
        .find_by_entity(claims.tenant_id, "vehicle", vehicle_id)
        .await?
        .ok_or(AppError::NotFound { resource: "VehicleComplianceProfile", id: vehicle_id.to_string() })?;

    let document_type_id = match req.document_type_id {
        Some(id) => id,
        None => {
            let code = req.document_type_code.as_deref().ok_or_else(|| AppError::Validation(
                "document_type_id or document_type_code is required".into(),
            ))?;
            state.compliance.doc_types
                .find_by_code(code)
                .await?
                .ok_or_else(|| AppError::NotFound { resource: "DocumentType", id: code.to_string() })?
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

/// GET /api/v1/compliance/vehicles/:vehicle_id/documents/:doc_id
/// Requires FLEET_READ or COMPLIANCE_REVIEW.
pub async fn get_vehicle_document(
    AuthClaims(claims): AuthClaims,
    Path((vehicle_id, doc_id)): Path<(Uuid, Uuid)>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    claims.require_any_permission(&[permissions::FLEET_READ, permissions::COMPLIANCE_REVIEW])?;

    let doc = state.compliance.documents.find_by_id(doc_id).await?
        .ok_or(AppError::NotFound { resource: "VehicleDocument", id: doc_id.to_string() })?;

    let profile = state.compliance.profiles.find_by_id(doc.compliance_profile_id).await?
        .ok_or(AppError::NotFound { resource: "VehicleComplianceProfile", id: doc.compliance_profile_id.to_string() })?;

    if profile.entity_id != vehicle_id || profile.tenant_id != claims.tenant_id {
        return Err(AppError::Forbidden { resource: "VehicleDocument".to_owned() });
    }

    Ok(Json(serde_json::json!({ "data": doc })))
}

/// GET /api/v1/compliance/vehicles/:vehicle_id/documents/:doc_id/url
/// Returns a presigned download URL. Requires FLEET_READ or COMPLIANCE_REVIEW.
pub async fn get_vehicle_document_url(
    AuthClaims(claims): AuthClaims,
    Path((vehicle_id, doc_id)): Path<(Uuid, Uuid)>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    claims.require_any_permission(&[permissions::FLEET_READ, permissions::COMPLIANCE_REVIEW])?;

    let doc = state.compliance.documents.find_by_id(doc_id).await?
        .ok_or(AppError::NotFound { resource: "VehicleDocument", id: doc_id.to_string() })?;

    let profile = state.compliance.profiles.find_by_id(doc.compliance_profile_id).await?
        .ok_or(AppError::NotFound { resource: "VehicleComplianceProfile", id: doc.compliance_profile_id.to_string() })?;

    if profile.entity_id != vehicle_id || profile.tenant_id != claims.tenant_id {
        return Err(AppError::Forbidden { resource: "VehicleDocument".to_owned() });
    }

    let url = state.storage.presign_url(&doc.file_url).await?;
    Ok(Json(serde_json::json!({ "data": { "url": url, "expires_in": 900 } })))
}
