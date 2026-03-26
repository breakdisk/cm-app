//! Axum request extractors shared across services.
//!
//! Provides typed extractors for common request data such as
//! request IDs, tenant context forwarding, and client metadata.

use axum::{
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
};

/// Extracts the `X-Request-ID` header value, or generates a fallback string.
/// Services should forward this through to downstream calls and logs.
#[derive(Debug, Clone)]
pub struct RequestId(pub String);

#[axum::async_trait]
impl<S> FromRequestParts<S> for RequestId
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let id = parts
            .headers
            .get("x-request-id")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned)
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        Ok(RequestId(id))
    }
}
