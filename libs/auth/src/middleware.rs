//! Axum middleware: validates Bearer JWT, injects Claims into request extensions.
//! Every service that needs auth mounts this layer on its router.

use axum::{
    extract::Request,
    http::{header::AUTHORIZATION, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use std::sync::Arc;
use crate::{claims::Claims, jwt::JwtService};

/// State injected into the middleware — holds the shared JwtService.
pub type AuthState = Arc<JwtService>;

/// Axum middleware that validates the Bearer token and adds Claims to extensions.
///
/// Usage:
/// ```rust
/// let app = Router::new()
///     .route("/shipments", get(list_shipments))
///     .layer(axum::middleware::from_fn_with_state(auth_state, require_auth));
/// ```
pub async fn require_auth(
    axum::extract::State(jwt): axum::extract::State<AuthState>,
    mut req: Request,
    next: Next,
) -> Response {
    let token = extract_bearer_token(req.headers());

    let token = match token {
        Some(t) => t,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": { "code": "MISSING_TOKEN", "message": "Authorization header required" } })),
            ).into_response();
        }
    };

    match jwt.validate_access_token(token) {
        Ok(data) => {
            req.extensions_mut().insert(data.claims);
            next.run(req).await
        }
        Err(e) => {
            let (code, msg) = match e {
                crate::error::AuthError::TokenExpired => ("TOKEN_EXPIRED", "Token has expired"),
                _ => ("TOKEN_INVALID", "Token is invalid"),
            };
            (
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": { "code": code, "message": msg } })),
            ).into_response()
        }
    }
}

/// Extractor: pulls validated Claims from request extensions.
/// Panics if `require_auth` middleware was not applied — this is intentional
/// (it means the route was misconfigured).
pub struct AuthClaims(pub Claims);

impl AuthClaims {
    /// Returns `Ok(())` if the token carries the given permission, otherwise
    /// `Err(AppError::Forbidden)`. Designed for use with the `?` operator inside handlers.
    pub fn require_permission(
        &self,
        permission: &'static str,
    ) -> Result<(), logisticos_errors::AppError> {
        if self.0.has_permission(permission) {
            Ok(())
        } else {
            Err(logisticos_errors::AppError::Forbidden { resource: permission.to_owned() })
        }
    }
}

impl std::ops::Deref for AuthClaims {
    type Target = Claims;
    fn deref(&self) -> &Claims { &self.0 }
}

#[axum::async_trait]
impl<S: Send + Sync> axum::extract::FromRequestParts<S> for AuthClaims {
    type Rejection = (StatusCode, Json<serde_json::Value>);

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<Claims>()
            .cloned()
            .map(AuthClaims)
            .ok_or_else(|| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": { "code": "AUTH_NOT_CONFIGURED", "message": "Auth middleware not mounted" } })),
                )
            })
    }
}

fn extract_bearer_token(headers: &axum::http::HeaderMap) -> Option<&str> {
    let value = headers.get(AUTHORIZATION)?.to_str().ok()?;
    value.strip_prefix("Bearer ")
}

/// Guard macro: check permission inside a handler, return 403 if missing.
/// Usage: `require_permission!(claims, SHIPMENT_CREATE);`
#[macro_export]
macro_rules! require_permission {
    ($claims:expr, $permission:expr) => {
        if !$claims.has_permission($permission) {
            return Err(logisticos_errors::AppError::Forbidden {
                resource: $permission.to_owned(),
            });
        }
    };
}
