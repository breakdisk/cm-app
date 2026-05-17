use axum::{
    middleware::Next,
    request::Request,
};

/// Placeholder for future middleware (rate limiting, circuit breaker, etc.)
pub async fn extract_tenant_context(
    request: Request,
    next: Next,
) -> Request {
    next.run(request).await
}
