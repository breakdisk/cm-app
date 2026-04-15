use axum::{
    body::Body,
    extract::Request,
    http::{HeaderValue, StatusCode},
    middleware::Next,
    response::Response,
};
use subtle::ConstantTimeEq;

/// Header carrying the shared secret for service-to-service calls that must
/// never be reachable through the public API gateway.
///
/// The landing app sets this when it forwards a Firebase ID token exchange;
/// any other caller (including the api-gateway, Traefik, or a browser) will
/// lack the secret and get a 401.
pub const HEADER: &str = "x-internal-secret";

#[derive(Clone)]
pub struct InternalSecret(pub String);

/// Middleware: compares the incoming `X-Internal-Secret` header against the
/// configured secret in constant time.
pub async fn require_internal_secret(
    axum::extract::State(expected): axum::extract::State<InternalSecret>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let provided: Option<&HeaderValue> = req.headers().get(HEADER);
    let provided_bytes = provided.and_then(|v| v.to_str().ok()).unwrap_or("");

    let expected_bytes = expected.0.as_bytes();
    // Empty expected secret must never match — fail closed.
    if expected_bytes.is_empty() || provided_bytes.is_empty() {
        return Ok(unauthorized());
    }
    // Length mismatch is safe to short-circuit; the caller already knows
    // their own input length, so no side channel is leaked.
    if provided_bytes.as_bytes().len() != expected_bytes.len() {
        return Ok(unauthorized());
    }
    if provided_bytes.as_bytes().ct_eq(expected_bytes).unwrap_u8() != 1 {
        return Ok(unauthorized());
    }

    Ok(next.run(req).await)
}

fn unauthorized() -> Response {
    Response::builder()
        .status(StatusCode::UNAUTHORIZED)
        .body(Body::from(r#"{"error":{"code":"internal_secret_required"}}"#))
        .unwrap()
}
