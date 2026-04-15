pub mod auth;
pub mod tenants;
pub mod users;
pub mod api_keys;
pub mod health;
pub mod push_tokens;

use axum::{Router, routing::{get, post, delete}};
use std::sync::Arc;
use crate::api::middleware::{require_internal_secret, InternalSecret};
use crate::application::services::{auth_service::AuthService, tenant_service::TenantService, api_key_service::ApiKeyService};

pub struct AppState {
    pub auth_service: Arc<AuthService>,
    pub tenant_service: Arc<TenantService>,
    pub api_key_service: Arc<ApiKeyService>,
    pub jwt: Arc<logisticos_auth::jwt::JwtService>,
    pub reset_token_repo: Arc<crate::infrastructure::db::user_repo::PgPasswordResetTokenRepository>,
    pub email_verification_token_repo: Arc<crate::infrastructure::db::user_repo::PgEmailVerificationTokenRepository>,
    pub push_token_repo: Arc<crate::infrastructure::db::push_token_repo::PgPushTokenRepository>,
}

pub fn router(state: Arc<AppState>) -> Router {
    let internal_secret_value = std::env::var("LOGISTICOS_INTERNAL_SECRET").unwrap_or_default();
    if internal_secret_value.is_empty() {
        tracing::warn!(
            "LOGISTICOS_INTERNAL_SECRET is not set — /v1/internal/* endpoints will reject every request"
        );
    }
    let internal_secret = InternalSecret(internal_secret_value);

    Router::new()
        // Health
        .route("/health", get(health::health))
        .route("/ready",  get(health::ready))
        .route("/metrics", get(health::metrics))
        // Auth (public — no JWT required)
        .route("/v1/auth/login",                    post(auth::login))
        .route("/v1/auth/refresh",                  post(auth::refresh))
        .route("/v1/auth/register",                 post(auth::register))
        .route("/v1/auth/forgot-password",          post(auth::forgot_password))
        .route("/v1/auth/reset-password",           post(auth::reset_password))
        .route("/v1/auth/send-verification-email",  post(auth::send_verification_email))
        .route("/v1/auth/verify-email",             post(auth::verify_email))
        // OTP auth (driver app + customer app)
        .route("/v1/auth/otp/send",                 post(auth::send_otp))
        .route("/v1/auth/otp/verify",               post(auth::verify_otp))
        // Tenant onboarding (public)
        .route("/v1/tenants", post(tenants::create_tenant))
        // Internal endpoints (protected by Docker network isolation, NOT exposed via api-gateway)
        .route("/internal/push-tokens", get(push_tokens::list_push_tokens_internal))
        .merge(internal_router(internal_secret))
        // Protected routes
        .nest("/v1", protected_router(state.clone()))
        .with_state(state)
}

/// Routes reachable only from the internal network with a shared secret.
/// Used by the landing app's server-side route handler to exchange a
/// verified Firebase ID token for a LogisticOS JWT. The api-gateway must
/// strip `X-Internal-Secret` on ingress so the header can never reach this
/// scope from the public internet.
fn internal_router(secret: InternalSecret) -> Router<Arc<AppState>> {
    let guard = axum::middleware::from_fn_with_state(secret, require_internal_secret);
    Router::new()
        .route("/v1/internal/auth/exchange-firebase", post(auth::exchange_firebase))
        .layer(guard)
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
        .route("/push-tokens",     post(push_tokens::register_push_token).delete(push_tokens::delete_push_token))
        .layer(auth_layer)
}
