pub mod auth;
pub mod tenants;
pub mod users;
pub mod api_keys;
pub mod health;

use axum::{Router, routing::{get, post, delete}};
use std::sync::Arc;
use crate::application::services::{auth_service::AuthService, tenant_service::TenantService, api_key_service::ApiKeyService};

pub struct AppState {
    pub auth_service: Arc<AuthService>,
    pub tenant_service: Arc<TenantService>,
    pub api_key_service: Arc<ApiKeyService>,
    pub jwt: Arc<logisticos_auth::jwt::JwtService>,
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        // Health
        .route("/health", get(health::health))
        .route("/ready",  get(health::ready))
        .route("/metrics", get(health::metrics))
        // Auth (public — no JWT required)
        .route("/v1/auth/login",   post(auth::login))
        .route("/v1/auth/refresh", post(auth::refresh))
        // Tenant onboarding (public)
        .route("/v1/tenants", post(tenants::create_tenant))
        // Protected routes
        .nest("/v1", protected_router(state.clone()))
        .with_state(state)
}

fn protected_router(state: Arc<AppState>) -> Router<Arc<AppState>> {
    let auth_layer = axum::middleware::from_fn_with_state(
        Arc::clone(&state.jwt),
        logisticos_auth::middleware::require_auth,
    );
    Router::new()
        .route("/users",           get(users::list_users).post(users::invite_user))
        .route("/users/:id",       get(users::get_user))
        .route("/api-keys",        get(api_keys::list).post(api_keys::create))
        .route("/api-keys/:id",    delete(api_keys::revoke))
        .layer(auth_layer)
}
