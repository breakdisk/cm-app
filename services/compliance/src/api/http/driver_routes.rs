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
        req.document_number,
        parse_date(req.issue_date)?,
        parse_date(req.expiry_date)?,
        req.file_url,
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
