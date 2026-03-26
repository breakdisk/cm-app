use std::sync::Arc;
use axum::{Router, routing::{get, post}};
use tower_http::trace::TraceLayer;
use logisticos_auth::jwt::JwtService;
use crate::application::services::ComplianceService;
use crate::infrastructure::storage::DocumentStorage;

pub struct AppState {
    pub compliance: Arc<ComplianceService>,
    pub jwt:        Arc<JwtService>,
    pub storage:    Arc<DocumentStorage>,
    pub pool:       sqlx::PgPool,   // for health check only
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(health::health))
        .route("/ready",  get(health::ready))
        .route("/api/v1/compliance/me/profile",                          get(driver_routes::get_my_profile))
        .route("/api/v1/compliance/me/documents",                        post(driver_routes::submit_document))
        .route("/api/v1/compliance/me/documents/:doc_id",                get(driver_routes::get_document))
        .route("/api/v1/compliance/me/documents/:doc_id/url",            get(driver_routes::get_document_url))
        .route("/api/v1/compliance/admin/queue",                          get(admin_routes::review_queue))
        .route("/api/v1/compliance/admin/profiles",                       get(admin_routes::list_profiles))
        .route("/api/v1/compliance/admin/profiles/:profile_id",           get(admin_routes::get_profile))
        .route("/api/v1/compliance/admin/documents/:doc_id/approve",      post(admin_routes::approve_document))
        .route("/api/v1/compliance/admin/documents/:doc_id/reject",       post(admin_routes::reject_document))
        .route("/api/v1/compliance/admin/profiles/:profile_id/suspend",   post(admin_routes::suspend_profile))
        .route("/api/v1/compliance/admin/profiles/:profile_id/reinstate", post(admin_routes::reinstate_profile))
        .route("/api/v1/compliance/internal/status/:entity_type/:entity_id", get(internal_routes::get_status))
        .with_state(state)
        .layer(TraceLayer::new_for_http())
}

mod health;
mod driver_routes;
mod admin_routes;
mod internal_routes;
