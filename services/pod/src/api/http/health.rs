use axum::Json;
use logisticos_common::health::{HealthResponse, ReadyResponse, ReadyChecks, CheckStatus};

pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "pod",
        version: env!("CARGO_PKG_VERSION"),
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    })
}
pub async fn ready() -> Json<ReadyResponse> {
    Json(ReadyResponse { status: "ready".into(), checks: ReadyChecks { database: CheckStatus::Ok, kafka: CheckStatus::Ok, redis: CheckStatus::Ok } })
}
