use axum::Json;
use serde_json::{json, Value};

pub async fn health() -> Json<Value> {
    Json(json!({ "status": "ok", "service": "identity" }))
}

pub async fn ready() -> Json<Value> {
    // In production: check DB pool, Redis connection
    Json(json!({ "status": "ready" }))
}

pub async fn metrics() -> &'static str {
    // In production: expose prometheus metrics via metrics-exporter-prometheus crate
    "# HELP identity_requests_total Total requests\n# TYPE identity_requests_total counter\n"
}
