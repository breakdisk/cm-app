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

    /// Returns `Ok(())` if the token carries **any** of the given permissions.
    pub fn require_any_permission(
        &self,
        permissions: &[&'static str],
    ) -> Result<(), logisticos_errors::AppError> {
        if permissions.iter().any(|p| self.0.has_permission(p)) {
            Ok(())
        } else {
            Err(logisticos_errors::AppError::Forbidden {
                resource: permissions.join(" | "),
            })
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

/// CSRF guard header: every browser/mobile caller must stamp one of these
/// values. Server-to-server callers inside the mesh use `service`.
pub const CLIENT_HEADER: &str = "x-logisticos-client";
pub const CLIENT_WEB: &str = "web";
pub const CLIENT_MOBILE: &str = "mobile";
pub const CLIENT_SERVICE: &str = "service";

/// CSRF defense middleware: rejects requests missing a valid
/// `X-LogisticOS-Client` header. Cookie-bearing auth (portals) relies on this
/// SameOrigin-esque custom-header trick to block cross-site form posts —
/// attackers can't set custom headers on cross-origin `fetch`/form submissions
/// without a preflight that the browser will refuse unless our CORS policy
/// allows it (which it does not for third-party origins).
///
/// Fail-closed: any unrecognized value yields 403. Mount on any router that
/// accepts mutating traffic authenticated by cookies.
pub async fn require_client_header(req: Request, next: Next) -> Response {
    let Some(value) = req
        .headers()
        .get(CLIENT_HEADER)
        .and_then(|v| v.to_str().ok())
    else {
        return client_header_rejection("CLIENT_HEADER_MISSING", "X-LogisticOS-Client header required");
    };

    if !matches!(value, CLIENT_WEB | CLIENT_MOBILE | CLIENT_SERVICE) {
        return client_header_rejection("CLIENT_HEADER_INVALID", "Unrecognized X-LogisticOS-Client value");
    }

    next.run(req).await
}

fn client_header_rejection(code: &'static str, msg: &'static str) -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(json!({ "error": { "code": code, "message": msg } })),
    )
        .into_response()
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
