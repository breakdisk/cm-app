use axum::{extract::{Path, State, Query}, Json};
use std::sync::Arc;
use uuid::Uuid;
use serde::Deserialize;
use logisticos_auth::{middleware::AuthClaims, rbac::permissions};
use logisticos_errors::AppError;
use crate::api::http::AppState;

#[derive(Deserialize)]
pub struct QueueParams { pub limit: Option<i64>, pub offset: Option<i64> }

pub async fn review_queue(
    AuthClaims(claims): AuthClaims,
    Query(params): Query<QueueParams>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    claims.require_permission(permissions::COMPLIANCE_REVIEW)?;
    let docs = state.compliance.documents
        .list_pending_review(Some(claims.tenant_id), params.limit.unwrap_or(50), params.offset.unwrap_or(0))
        .await?;
    Ok(Json(serde_json::json!({ "data": docs })))
}

pub async fn list_profiles(
    AuthClaims(claims): AuthClaims,
    Query(params): Query<QueueParams>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    claims.require_permission(permissions::COMPLIANCE_REVIEW)?;
    let profiles = state.compliance.profiles
        .list_by_tenant(claims.tenant_id, None, params.limit.unwrap_or(100), params.offset.unwrap_or(0))
        .await?;
    Ok(Json(serde_json::json!({ "data": profiles })))
}

pub async fn get_profile(
    AuthClaims(claims): AuthClaims,
    Path(profile_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    claims.require_permission(permissions::COMPLIANCE_REVIEW)?;
    let profile = state.compliance.profiles.find_by_id(profile_id).await?
        .ok_or(AppError::NotFound { resource: "ComplianceProfile", id: profile_id.to_string() })?;
    if profile.tenant_id != claims.tenant_id {
        return Err(AppError::Forbidden { resource: "ComplianceProfile".to_owned() });
    }
    let docs  = state.compliance.documents.list_by_profile(profile_id).await?;
    let audit = state.compliance.audit.list_by_profile(profile_id, 100, 0).await?;
    Ok(Json(serde_json::json!({ "data": { "profile": profile, "documents": docs, "audit_log": audit } })))
}

#[derive(Deserialize)]
pub struct RejectRequest { pub reason: String }

pub async fn approve_document(
    AuthClaims(claims): AuthClaims,
    Path(doc_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    claims.require_permission(permissions::COMPLIANCE_REVIEW)?;
    // Verify document belongs to caller's tenant
    let doc = state.compliance.documents.find_by_id(doc_id).await?
        .ok_or(AppError::NotFound { resource: "DriverDocument", id: doc_id.to_string() })?;
    let profile = state.compliance.profiles.find_by_id(doc.compliance_profile_id).await?
        .ok_or(AppError::NotFound { resource: "ComplianceProfile", id: doc.compliance_profile_id.to_string() })?;
    if profile.tenant_id != claims.tenant_id {
        return Err(AppError::Forbidden { resource: "DriverDocument".to_owned() });
    }
    state.compliance.review_document(doc_id, true, None, claims.user_id).await?;
    Ok(Json(serde_json::json!({ "data": { "ok": true } })))
}

pub async fn reject_document(
    AuthClaims(claims): AuthClaims,
    Path(doc_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
    Json(req): Json<RejectRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    claims.require_permission(permissions::COMPLIANCE_REVIEW)?;
    // Verify document belongs to caller's tenant
    let doc = state.compliance.documents.find_by_id(doc_id).await?
        .ok_or(AppError::NotFound { resource: "DriverDocument", id: doc_id.to_string() })?;
    let profile = state.compliance.profiles.find_by_id(doc.compliance_profile_id).await?
        .ok_or(AppError::NotFound { resource: "ComplianceProfile", id: doc.compliance_profile_id.to_string() })?;
    if profile.tenant_id != claims.tenant_id {
        return Err(AppError::Forbidden { resource: "DriverDocument".to_owned() });
    }
    state.compliance.review_document(doc_id, false, Some(req.reason), claims.user_id).await?;
    Ok(Json(serde_json::json!({ "data": { "ok": true } })))
}

#[derive(Deserialize)]
pub struct SuspendRequest { pub reason: Option<String> }

pub async fn suspend_profile(
    AuthClaims(claims): AuthClaims,
    Path(profile_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
    Json(req): Json<SuspendRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    claims.require_permission(permissions::COMPLIANCE_ADMIN)?;
    state.compliance.suspend(profile_id, claims.user_id, req.reason).await?;
    Ok(Json(serde_json::json!({ "data": { "ok": true } })))
}

pub async fn reinstate_profile(
    AuthClaims(claims): AuthClaims,
    Path(profile_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    claims.require_permission(permissions::COMPLIANCE_ADMIN)?;
    state.compliance.reinstate(profile_id, claims.user_id, None).await?;
    Ok(Json(serde_json::json!({ "data": { "ok": true } })))
}
