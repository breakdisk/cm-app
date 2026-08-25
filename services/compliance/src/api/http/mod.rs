use std::sync::Arc;
use axum::{Router, extract::DefaultBodyLimit, routing::{get, post}};
use tower_http::trace::TraceLayer;
use logisticos_auth::jwt::JwtService;
use crate::application::services::ComplianceService;
use crate::infrastructure::storage::DocumentStorage;

/// Max request body for KYC document uploads. The storage layer caps files at
/// 10 MB; base64-in-JSON inflates that by ~33% (~13.3 MB) plus metadata, so the
/// route needs a limit well above Axum's 2 MB default (which otherwise rejects
/// every real photo with 413 before the handler runs). 16 MB matches the API
/// gateway's proxy body cap.
const MAX_UPLOAD_BODY_BYTES: usize = 16 * 1024 * 1024;

pub struct AppState {
    pub compliance: Arc<ComplianceService>,
    pub jwt:        Arc<JwtService>,
    pub storage:    Arc<DocumentStorage>,
    pub pool:       sqlx::PgPool,   // for health check only
}

/// Public router: health probes only — no auth required.
pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(health::health))
        .route("/ready",  get(health::ready))
        .merge(protected_router(state.clone()))
        .with_state(state)
        .layer(TraceLayer::new_for_http())
}

/// Protected router: all /api/v1/* routes require a valid Bearer JWT.
/// The auth layer injects Claims into request extensions; handlers pull them
/// out via the AuthClaims extractor. Without this layer the extractor returns
/// 500 "Auth middleware not mounted".
fn protected_router(state: Arc<AppState>) -> Router<Arc<AppState>> {
    let auth_layer = axum::middleware::from_fn_with_state(
        Arc::clone(&state.jwt),
        logisticos_auth::middleware::require_auth,
    );

    Router::new()
        // ── Customer / driver self-service routes ───────────────────────────
        .route("/api/v1/compliance/me/profile",
            get(driver_routes::get_my_profile))
        .route("/api/v1/compliance/me/documents",
            post(driver_routes::submit_document))
        .route("/api/v1/compliance/me/documents/upload",
            post(driver_routes::upload_document)
                // Scoped to this route only — the default 2 MB limit stays in
                // force everywhere else to keep the DoS surface small.
                .layer(DefaultBodyLimit::max(MAX_UPLOAD_BODY_BYTES)))
        // Presigned R2 upload flow (preferred for customer-app):
        //   POST upload-url  → get presigned PUT URL + s3_key
        //   PUT  <R2 url>    → client uploads file directly to R2
        //   POST confirm     → register the completed upload as a DriverDocument
        .route("/api/v1/compliance/me/documents/upload-url",
            post(driver_routes::get_kyc_upload_url))
        .route("/api/v1/compliance/me/documents/confirm",
            post(driver_routes::confirm_document))
        .route("/api/v1/compliance/me/documents/:doc_id",
            get(driver_routes::get_document))
        .route("/api/v1/compliance/me/documents/:doc_id/url",
            get(driver_routes::get_document_url))
        // ── Admin back-office routes ────────────────────────────────────────
        .route("/api/v1/compliance/admin/queue",
            get(admin_routes::review_queue))
        // The catalogue, so the console can name a document instead of printing
        // a slice of its uuid.
        .route("/api/v1/compliance/admin/document-types",
            get(admin_routes::list_document_types))
        // A link the reviewer can open. `file_url` is `s3://…`, which is inert
        // in a browser, and the `/me` presigner refuses anyone but the owner —
        // so without this route Approve and Reject are blind.
        .route("/api/v1/compliance/admin/documents/:doc_id/url",
            get(admin_routes::document_url))
        .route("/api/v1/compliance/admin/profiles",
            get(admin_routes::list_profiles))
        .route("/api/v1/compliance/admin/profiles/:profile_id",
            get(admin_routes::get_profile))
        .route("/api/v1/compliance/admin/documents/:doc_id/approve",
            post(admin_routes::approve_document))
        .route("/api/v1/compliance/admin/documents/:doc_id/reject",
            post(admin_routes::reject_document))
        .route("/api/v1/compliance/admin/profiles/:profile_id/suspend",
            post(admin_routes::suspend_profile))
        .route("/api/v1/compliance/admin/profiles/:profile_id/reinstate",
            post(admin_routes::reinstate_profile))
        // ── Internal mesh routes ────────────────────────────────────────────
        .route("/api/v1/compliance/internal/status/:entity_type/:entity_id",
            get(internal_routes::get_status))
        .layer(auth_layer)
}

mod health;
mod driver_routes;
mod admin_routes;
mod internal_routes;
