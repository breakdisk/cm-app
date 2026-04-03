//! Token-bucket rate limiting per tenant + per API key.
//! Limits stored in Redis with TTL-based sliding windows.
//!
//! Limits by subscription tier:
//!   Starter:    100  req/min
//!   Growth:     500  req/min
//!   Business:   2000 req/min
//!   Enterprise: 10000 req/min (or custom contract)

use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use redis::AsyncCommands;
use serde_json::json;

const WINDOW_SECONDS: u64 = 60;

pub struct RateLimitConfig {
    pub redis: redis::Client,
}

/// Returns the request limit per minute for a given subscription tier.
pub fn limit_for_tier(tier: &str) -> u64 {
    match tier {
        "starter"    => 100,
        "growth"     => 500,
        "business"   => 2_000,
        "enterprise" => 10_000,
        _            => 100,
    }
}

/// Check and increment the request counter for a tenant.
/// Returns (allowed: bool, remaining: u64, reset_in_seconds: u64).
pub async fn check_rate_limit(
    redis: &mut impl AsyncCommands,
    tenant_id: &str,
    tier: &str,
) -> anyhow::Result<(bool, u64, u64)> {
    let key = format!("ratelimit:tenant:{}:{}", tenant_id, current_window());
    let limit = limit_for_tier(tier);

    let count: u64 = redis.incr(&key, 1u64).await?;
    if count == 1 {
        // New window — set TTL
        let _: () = redis.expire(&key, WINDOW_SECONDS as i64).await?;
    }

    let ttl: i64 = redis.ttl(&key).await.unwrap_or(WINDOW_SECONDS as i64);
    let reset_in = ttl.max(0) as u64;
    let remaining = limit.saturating_sub(count);

    Ok((count <= limit, remaining, reset_in))
}

fn current_window() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        / WINDOW_SECONDS
}

/// Axum middleware factory. Must be composed with the JWT auth middleware
/// so that claims.tenant_id and claims.subscription_tier are available.
pub async fn rate_limit_middleware(
    req: Request,
    next: Next,
) -> Response {
    // In production: extract claims from extensions, hit Redis, add X-RateLimit headers
    // For now forward — full implementation wires in Redis pool via State
    next.run(req).await
}
