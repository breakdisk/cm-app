use axum::{extract::{Path, State, Query}, Json};
use std::sync::Arc;
use uuid::Uuid;
use serde::Deserialize;
use logisticos_auth::{middleware::AuthClaims, rbac::permissions, require_permission};
use logisticos_errors::AppError;
use crate::api::http::AppState;
use crate::domain::entities::{ComplianceAuditLog, ComplianceProfile, DriverDocument};

#[derive(Deserialize)]
pub struct QueueParams { pub limit: Option<i64>, pub offset: Option<i64> }

/// Resolve a document an admin named, and prove it belongs to their tenant.
///
/// Three steps — document, then its profile, then the tenant comparison — and
/// every admin route that touches a document by id needs all three. `approve`
/// and `reject` each open-coded them, and the presign route below would have
/// been the third copy of a check where a copy that drifts is a cross-tenant
/// read of someone's identity document.
///
/// A document in another tenant is `Forbidden` rather than `NotFound`. That is
/// this service's existing convention on these routes and it is kept rather
/// than quietly changed here; field-ops chose the opposite for its courier
/// routes, deliberately, so ids there cannot be probed.
async fn authorize_document(
    state:  &AppState,
    claims: &logisticos_auth::claims::Claims,
    doc_id: Uuid,
) -> Result<(DriverDocument, ComplianceProfile), AppError> {
    let doc = state.compliance.documents.find_by_id(doc_id).await?
        .ok_or(AppError::NotFound { resource: "DriverDocument", id: doc_id.to_string() })?;
    let profile = state.compliance.profiles.find_by_id(doc.compliance_profile_id).await?
        .ok_or(AppError::NotFound {
            resource: "ComplianceProfile",
            id: doc.compliance_profile_id.to_string(),
        })?;
    if profile.tenant_id != claims.tenant_id {
        return Err(AppError::Forbidden { resource: "DriverDocument".to_owned() });
    }
    Ok((doc, profile))
}

/// Can this `file_url` be turned into a link a browser can open?
///
/// Everything this service stores is written as `s3://bucket/key` — by
/// `DocumentStorage::upload` and by `confirm_document` alike. Rows that are not
/// exist: the seeded mocks use `#`, and `submit_document` accepts a plain
/// `http(s)://` URL from a caller that hosted the file itself.
///
/// Those are already openable and must not be routed through the presigner,
/// which would fail on the missing prefix and read to the reviewer as a broken
/// document rather than a differently-stored one.
pub(crate) fn is_presignable(file_url: &str) -> bool {
    file_url.starts_with("s3://")
}

pub async fn review_queue(
    AuthClaims(claims): AuthClaims,
    Query(params): Query<QueueParams>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, permissions::COMPLIANCE_REVIEW);
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
    require_permission!(claims, permissions::COMPLIANCE_REVIEW);
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
    require_permission!(claims, permissions::COMPLIANCE_REVIEW);
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
    require_permission!(claims, permissions::COMPLIANCE_REVIEW);
    authorize_document(&state, &claims, doc_id).await?;
    state.compliance.review_document(doc_id, true, None, claims.user_id).await?;
    Ok(Json(serde_json::json!({ "data": { "ok": true } })))
}

pub async fn reject_document(
    AuthClaims(claims): AuthClaims,
    Path(doc_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
    Json(req): Json<RejectRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, permissions::COMPLIANCE_REVIEW);
    authorize_document(&state, &claims, doc_id).await?;
    state.compliance.review_document(doc_id, false, Some(req.reason), claims.user_id).await?;
    Ok(Json(serde_json::json!({ "data": { "ok": true } })))
}

/// `GET /admin/documents/:doc_id/url` — a link the reviewer can actually open.
///
/// The gap this closes: `driver_documents.file_url` holds `s3://bucket/key`, and
/// the console rendered it straight into an `<a href>`. A browser does nothing
/// with an `s3://` href, and the only presigning route was `/me/documents/:id/url`,
/// which requires `profile.entity_id == claims.user_id` and so answered 403 to
/// every admin. Approve and Reject were decisions taken without sight of the
/// document.
///
/// The audit row is the reason this is a route rather than a URL folded into the
/// profile payload: reading someone's identity document is a privacy-relevant
/// act under PDPA/GDPR, and one row per deliberate open is evidence, whereas one
/// per panel render is noise. It is written before the URL is minted and its
/// failure is propagated — an unlogged view of a licence is not a thing this
/// service should hand out.
pub async fn document_url(
    AuthClaims(claims): AuthClaims,
    Path(doc_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, permissions::COMPLIANCE_REVIEW);
    let (doc, profile) = authorize_document(&state, &claims, doc_id).await?;

    if !is_presignable(&doc.file_url) {
        return Err(AppError::Validation(
            "This document is not stored in object storage and has no presigned link.".into(),
        ));
    }

    state.compliance.audit.append(&ComplianceAuditLog {
        id:                    Uuid::new_v4(),
        tenant_id:             profile.tenant_id,
        compliance_profile_id: profile.id,
        document_id:           Some(doc.id),
        event_type:            "doc_viewed".into(),
        actor_id:              claims.user_id,
        actor_type:            "admin".into(),
        notes:                 None,
        created_at:            chrono::Utc::now(),
    }).await?;

    let url = state.storage.presign_url(&doc.file_url).await?;
    Ok(Json(serde_json::json!({
        "data": { "url": url, "expires_in": crate::infrastructure::storage::PRESIGN_TTL_SECS }
    })))
}

/// `GET /admin/document-types` — the catalogue, so the console can name a
/// document instead of printing the first twelve characters of its uuid.
///
/// Read-only and not tenant-scoped, because the table is not: it is seeded by
/// migration and identical in every environment. `compliance:review` still gates
/// it — it describes what a jurisdiction demands of its couriers, which is not
/// something to hand to an unauthenticated caller.
pub async fn list_document_types(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, permissions::COMPLIANCE_REVIEW);
    let types = state.compliance.doc_types.list_all().await?;
    Ok(Json(serde_json::json!({ "data": types })))
}

#[derive(Deserialize)]
pub struct SuspendRequest { pub reason: Option<String> }

pub async fn suspend_profile(
    AuthClaims(claims): AuthClaims,
    Path(profile_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
    Json(req): Json<SuspendRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, permissions::COMPLIANCE_ADMIN);
    state.compliance.suspend(profile_id, claims.user_id, req.reason).await?;
    Ok(Json(serde_json::json!({ "data": { "ok": true } })))
}

pub async fn reinstate_profile(
    AuthClaims(claims): AuthClaims,
    Path(profile_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, permissions::COMPLIANCE_ADMIN);
    state.compliance.reinstate(profile_id, claims.user_id, None).await?;
    Ok(Json(serde_json::json!({ "data": { "ok": true } })))
}

#[cfg(test)]
mod tests {
    use super::is_presignable;

    /// Both write paths in this service produce this shape:
    /// `DocumentStorage::upload` returns it, and `confirm_document` formats it.
    #[test]
    fn an_object_this_service_stored_is_presignable() {
        assert!(is_presignable(
            "s3://logisticos-compliance/compliance/00000000-0000-0000-0000-000000000001/abc",
        ));
    }

    /// `submit_document` accepts an `http(s)://` URL from a caller that hosted
    /// the file itself. Presigning one would fail on the missing `s3://` prefix
    /// and read to the reviewer as a broken document rather than a link they can
    /// already open.
    #[test]
    fn a_caller_hosted_url_is_left_alone() {
        assert!(!is_presignable("https://example.test/licence.jpg"));
        assert!(!is_presignable("http://example.test/licence.jpg"));
    }

    /// The seeded console mocks use `#`. Handing that to the presigner produces
    /// an error where a disabled button belongs.
    #[test]
    fn a_placeholder_is_not_presignable() {
        assert!(!is_presignable("#"));
        assert!(!is_presignable(""));
    }

    /// The check is on the scheme, not on a substring of it — a URL that merely
    /// mentions `s3://` later is not an object key.
    #[test]
    fn the_scheme_must_start_the_url() {
        assert!(!is_presignable("https://proxy.test/?target=s3://bucket/key"));
    }
}
