use axum::Json;
use logisticos_common::health::{HealthResponse, ReadyResponse, ReadyChecks, CheckStatus};

pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".into(),
        service: "driver-ops".into(),
        version: env!("CARGO_PKG_VERSION").into(),
    })
}

pub async fn ready() -> Json<ReadyResponse> {
    Json(ReadyResponse {
        status: "ready".into(),
        checks: ReadyChecks {
            database: CheckStatus::Ok,
            kafka:    CheckStatus::Ok,
            redis:    CheckStatus::Ok,
        },
    })
}
