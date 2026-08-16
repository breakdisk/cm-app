use axum::Json;
use logisticos_common::health::{HealthResponse, ReadyResponse, ReadyChecks, CheckStatus};

pub async fn health() -> Json<HealthResponse> {
    use std::time::{SystemTime, UNIX_EPOCH};
    Json(HealthResponse {
        status: "ok",
        service: "driver-ops",
        version: env!("CARGO_PKG_VERSION"),
        timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
    })
}

pub async fn ready() -> Json<ReadyResponse> {
    Json(ReadyResponse {
        status: "ready",
        checks: ReadyChecks {
            database: CheckStatus::Ok,
            kafka:    CheckStatus::Ok,
            redis:    CheckStatus::Ok,
        },
    })
}
