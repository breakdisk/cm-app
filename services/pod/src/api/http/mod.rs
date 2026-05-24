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
        .with_state(state)
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
        .route("/pops",                          post(pod::initiate_pickup))
        .route("/pops/:id/upload-url",           post(pod::get_pop_upload_url))
        .route("/pops/:id/submit",               put(pod::submit_pickup))
        .route("/pops/:id",                      get(pod::get_pop))
        // OTP management
        .route("/otps/generate",                 post(pod::generate_otp))
        .route("/otps/verify",                   post(pod::verify_otp))
        .layer(auth_layer)
}
