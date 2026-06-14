pub mod pod;
pub mod health;

use axum::{Router, routing::{get, post, put}};
use std::sync::Arc;
use crate::application::services::PodService;

pub struct AppState {
    pub pod_service: Arc<PodService>,
    pub jwt: Arc<logisticos_auth::jwt::JwtService>,
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(health::health))
        .route("/ready",  get(health::ready))
        .nest("/v1", protected_router(state.clone()))
        .nest("/v1", internal_router())
        .with_state(state)
}

/// Internal service-to-service routes — no JWT, protected by Istio mTLS in production.
fn internal_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/internal/pop-status",           get(pod::pop_status_internal))
        .route("/internal/pop-evidence/:shipment_id", get(pod::get_evidence_internal))
}

fn protected_router(state: Arc<AppState>) -> Router<Arc<AppState>> {
    let auth_layer = axum::middleware::from_fn_with_state(
        Arc::clone(&state.jwt),
        logisticos_auth::middleware::require_auth,
    );
    Router::new()
        // POD lifecycle
        .route("/pods",                          post(pod::initiate).get(pod::list_pods))
        .route("/pods/:id/signature",            put(pod::attach_signature))
        .route("/pods/:id/upload-url",           post(pod::get_upload_url))
        .route("/pods/:id/photos",               post(pod::attach_photo))
        .route("/pods/:id/submit",               put(pod::submit))
        .route("/pods/:id",                      get(pod::get_pod))
        // POP lifecycle (Proof of Pickup — chain-of-custody open)
        .route("/pops",                          post(pod::initiate_pickup).get(pod::list_pops))
        .route("/pops/:id/upload-url",           post(pod::get_pop_upload_url))
        .route("/pops/:id/submit",               put(pod::submit_pickup))
        .route("/pops/:id",                      get(pod::get_pop))
        // Gateway-compat aliases: old API gateway images route /v1/pod* → pod service
        // but have no entry for /v1/pops. These aliases start with /v1/pod so the
        // stale gateway forwards them here without needing a gateway redeploy.
        .route("/pod/pops",                      post(pod::initiate_pickup).get(pod::list_pops))
        .route("/pod/pops/:id/upload-url",       post(pod::get_pop_upload_url))
        .route("/pod/pops/:id/submit",           put(pod::submit_pickup))
        .route("/pod/pops/:id",                  get(pod::get_pop))
        // OTP management
        .route("/otps/generate",                 post(pod::generate_otp))
        .route("/otps/verify",                   post(pod::verify_otp))
        .layer(auth_layer)
}
