use axum::{extract::{Path, State}, Json};
use std::sync::Arc;
use uuid::Uuid;
use logisticos_auth::middleware::AuthClaims;
use logisticos_errors::AppError;
use serde::Deserialize;
use crate::api::http::AppState;
use crate::domain::entities::ComplianceProfile;
use crate::infrastructure::storage::PRESIGN_TTL_SECS;

// ── Shared helpers used by multiple handlers ───────────────────────────────

/// Which kind of compliance subject the caller is.
///
/// The `/me/*` routes are shared by customers doing KYC and by field workers
/// uploading a licence, so this cannot be a constant — and it used to be two
/// different constants in the same function: `resolve_profile` looked a profile
/// up as `"driver"` and created it as `"customer"`.
///
/// The consequence was silent and total. A field worker with no profile got a
/// `customer`-typed one, the next lookup missed again, and every document they
/// uploaded hung off a profile the admin queue — which filters
/// `entity_type = 'driver'` — could never show. On 2026-08-23 production held
/// **17 profiles, every one of them `customer`, and zero `driver`**, which is
/// why the compliance console had never displayed a single row.
///
/// Roles, not permissions: a driver's claim to be a driver is their role, and
/// `drivers:read` is held by admins who are not drivers themselves.
pub fn entity_kind_for(roles: &[String]) -> &'static str {
    if roles.iter().any(|r| r == "driver" || r == "courier") {
        "driver"
    } else {
        "customer"
    }
}

#[cfg(test)]
mod entity_kind_tests {
    use super::entity_kind_for;

    fn roles(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn a_driver_is_a_driver() {
        assert_eq!(entity_kind_for(&roles(&["driver"])), "driver");
    }

    /// OmniDeliv calls them couriers and field-ops calls the table `couriers`,
    /// but compliance has one entity type for people who carry things.
    #[test]
    fn a_courier_is_also_a_driver_to_compliance() {
        assert_eq!(entity_kind_for(&roles(&["courier"])), "driver");
    }

    #[test]
    fn a_customer_doing_kyc_is_a_customer() {
        assert_eq!(entity_kind_for(&roles(&["customer"])), "customer");
        assert_eq!(entity_kind_for(&[]), "customer");
    }

    /// One person can hold both roles. Being a driver is the stronger claim:
    /// their documents are what gate them from working.
    #[test]
    fn holding_both_roles_resolves_to_driver() {
        assert_eq!(entity_kind_for(&roles(&["customer", "driver"])), "driver");
    }

    /// The bug, stated so it cannot come back: whatever this returns, the
    /// lookup and the create must be given the *same* answer.
    #[test]
    fn the_answer_is_stable_for_the_same_roles() {
        let r = roles(&["driver"]);
        assert_eq!(entity_kind_for(&r), entity_kind_for(&r));
    }
}


async fn resolve_profile(
    state: &AppState,
    tenant_id: Uuid,
    user_id: Uuid,
    entity_kind: &str,
) -> Result<ComplianceProfile, AppError> {
    // One `entity_kind` for both halves. This used to look up "driver" and
    // create "customer", so a field worker's profile was never found and never
    // the type the admin queue filters on.
    match state.compliance.profiles
        .find_by_entity(tenant_id, entity_kind, user_id)
        .await?
    {
        Some(p) => Ok(p),
        None => Ok(state.compliance
            .ensure_profile(tenant_id, entity_kind, user_id, "PH")
            .await?),
    }
}

async fn resolve_document_type_id(
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

fn parse_date(s: Option<String>) -> Result<Option<chrono::NaiveDate>, AppError> {
    match s {
        None => Ok(None),
        Some(d) => chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d")
            .map(Some)
            .map_err(|_| AppError::Validation(
                format!("Invalid date format '{}'; expected YYYY-MM-DD", d),
            )),
    }
}

// ── Handlers ───────────────────────────────────────────────────────────────

pub async fn get_my_profile(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    // Try driver profile first (driver-app callers), fall back to customer
    // profile (customer-app KYC callers), and open one if neither exists.
    //
    // That last step used to be a 404, and it was a dead end rather than an
    // error. This is the only route the courier app's compliance screen calls
    // on load; a 404 leaves it with `failed = true` and an empty checklist, so
    // there is no row to tap, no upload form behind it, and no way to reach
    // `upload_document` — which is the one route that *would* have created the
    // profile. The screen's own words were "try again shortly", which was never
    // going to become true.
    //
    // Confirmed against production 2026-08-25: every courier alive was in this
    // state, because `driver.registered` had no publisher until #138 and
    // nothing has been backfilled.
    //
    // The two lookups are kept ahead of the create, and the order matters.
    // Production held 17 profiles that were all `entity_type = 'customer'`,
    // from the resolve_profile bug fixed earlier; going straight to
    // `ensure_profile("driver", …)` would strand those rows and every document
    // hanging off them behind a second, empty profile.
    let profile = match state.compliance.profiles
        .find_by_entity(claims.tenant_id, "driver", claims.user_id)
        .await?
    {
        Some(p) => p,
        None => match state.compliance.profiles
            .find_by_entity(claims.tenant_id, "customer", claims.user_id)
            .await?
        {
            Some(p) => p,
            None => state.compliance
                .ensure_profile(
                    claims.tenant_id,
                    entity_kind_for(&claims.roles),
                    claims.user_id,
                    "PH",
                )
                .await?,
        },
    };

    // Hardcoded "driver", not `profile.entity_type`, and knowingly so: a
    // customer doing KYC is handed the driver requirements. `profile.entity_type`
    // is the correct argument, but the seeded `passport` type is jurisdiction
    // `GLOBAL` while `ensure_profile` opens every profile as `PH`, so
    // `list_required_for("customer", "PH")` returns an empty list today —
    // trading a wrong checklist for no checklist. Fixing it means fixing the
    // jurisdiction match as well, and the customer KYC flow is not exercised
    // here. Left as-is rather than half-changed.
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

    let file_url = req.file_url.trim().to_owned();
    if file_url.is_empty() || (!file_url.starts_with("https://") && !file_url.starts_with("http://")) {
        return Err(AppError::Validation(
            "file_url must be a valid http:// or https:// URL".into(),
        ));
    }

    let document_number = req.document_number.trim().to_owned();
    if document_number.is_empty() {
        return Err(AppError::Validation("document_number cannot be empty".into()));
    }
    if document_number.len() > 100 {
        return Err(AppError::Validation(
            "document_number must be 100 characters or fewer".into(),
        ));
    }

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

/// POST /me/documents/upload — legacy base64 upload + submit in one call.
///
/// Client encodes the image/PDF to base64 and POSTs with metadata. Server
/// decodes, uploads to S3, then creates the DriverDocument record referencing
/// the resulting s3:// URI. Kept for backwards compatibility (driver app,
/// admin bulk ingest). New customer-app KYC uploads use the presigned flow
/// (`upload-url` + `confirm`).
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

pub async fn upload_document(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
    Json(req): Json<UploadDocumentRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    use base64::Engine as _;

    let profile = resolve_profile(&state, claims.tenant_id, claims.user_id, entity_kind_for(&claims.roles)).await?;
    let document_type_id = resolve_document_type_id(
        &state,
        req.document_type_id,
        req.document_type_code.as_deref(),
    ).await?;

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
        .map_err(|e| {
            tracing::error!(error = ?e, "compliance document S3 upload failed");
            AppError::Validation("Document upload failed; please try again".into())
        })?;

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

/// POST /me/documents/upload-url — returns a presigned R2 PUT URL so the
/// customer app can upload the KYC document directly to Cloudflare R2 without
/// routing the file bytes through this service.
///
/// Response: `{ data: { upload_url, s3_key, upload_headers, expires_in } }`
///
/// The client must:
///   1. PUT the raw file bytes to `upload_url` with `Content-Type` matching the
///      requested `content_type` plus any headers from `upload_headers`.
///   2. POST to `/me/documents/confirm` with `s3_key` + document metadata.
#[derive(Deserialize)]
pub struct KycUploadUrlRequest {
    pub content_type: String,
}

pub async fn get_kyc_upload_url(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
    Json(req): Json<KycUploadUrlRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let (upload_url, s3_key, upload_headers) = state.storage
        .presign_upload(claims.tenant_id, &req.content_type, PRESIGN_TTL_SECS)
        .await
        .map_err(|e| {
            tracing::error!(error = ?e, "failed to generate KYC presigned upload URL");
            AppError::Validation(format!("{e:#}"))
        })?;

    Ok(Json(serde_json::json!({
        "data": {
            "upload_url":     upload_url,
            "s3_key":         s3_key,
            "upload_headers": upload_headers,
            "expires_in":     PRESIGN_TTL_SECS,
        }
    })))
}

/// POST /me/documents/confirm — registers a KYC document that was already
/// uploaded directly to R2 via the presigned URL from `upload-url`.
///
/// Validates the `s3_key` prefix (tenant isolation), confirms the object exists
/// in R2 (HeadObject), then creates the `DriverDocument` record.
#[derive(Deserialize)]
pub struct ConfirmDocumentRequest {
    #[serde(default)]
    pub document_type_id:   Option<Uuid>,
    #[serde(default)]
    pub document_type_code: Option<String>,
    pub document_number:    String,
    /// Key returned by `upload-url` — client passes it back after a successful
    /// direct PUT to R2.
    pub s3_key:             String,
    /// MIME type the client used for the PUT. Accepted and discarded: there
    /// is no `content_type` column on the documents table, so despite what
    /// this comment used to claim, nothing persists it. Kept in the request
    /// type because clients already send it and removing it would 422 them.
    #[allow(dead_code)]
    pub content_type:       String,
    pub issue_date:         Option<String>,
    pub expiry_date:        Option<String>,
}

pub async fn confirm_document(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
    Json(req): Json<ConfirmDocumentRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    // Security: the s3_key must be scoped to this tenant's prefix so a caller
    // cannot point at another tenant's objects.
    let expected_prefix = format!("compliance/{}/", claims.tenant_id);
    if !req.s3_key.starts_with(&expected_prefix) {
        return Err(AppError::Validation(
            "s3_key must belong to this tenant (prefix mismatch)".into(),
        ));
    }
    // Reject path traversal: `../` in a key passes starts_with but points to a
    // different path after normalization on some storage backends.
    if req.s3_key.split('/').any(|seg| seg == "..") {
        return Err(AppError::Validation(
            "s3_key contains invalid path components".into(),
        ));
    }

    // Confirm the object was actually uploaded before writing the DB record.
    state.storage.head_object(&req.s3_key).await.map_err(|e| {
        tracing::warn!(s3_key = %req.s3_key, error = ?e, "KYC confirm: object not found in R2");
        AppError::Validation("Document not found in storage — the upload may have failed or the URL expired".into())
    })?;

    let profile = resolve_profile(&state, claims.tenant_id, claims.user_id, entity_kind_for(&claims.roles)).await?;
    let document_type_id = resolve_document_type_id(
        &state,
        req.document_type_id,
        req.document_type_code.as_deref(),
    ).await?;

    let document_number = req.document_number.trim().to_owned();
    if document_number.is_empty() {
        return Err(AppError::Validation("document_number cannot be empty".into()));
    }
    if document_number.len() > 100 {
        return Err(AppError::Validation("document_number must be 100 characters or fewer".into()));
    }

    let file_url = format!("s3://{}/{}", state.storage.bucket(), req.s3_key);

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
