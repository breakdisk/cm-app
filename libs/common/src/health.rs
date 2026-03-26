//! Standard health and readiness check handlers.
//! Mount these on every service at /health, /ready, /metrics.

use axum::Json;
use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub service: &'static str,
    pub version: &'static str,
    pub timestamp: u64,
}

#[derive(Debug, Serialize)]
pub struct ReadyResponse {
    pub status: &'static str,
    pub checks: ReadyChecks,
}

#[derive(Debug, Serialize)]
pub struct ReadyChecks {
    pub database: CheckStatus,
    pub redis: CheckStatus,
    pub kafka: CheckStatus,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus { Ok, Degraded, Down }

pub fn health_handler(service: &'static str) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service,
        version: env!("CARGO_PKG_VERSION"),
        timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
    })
}
