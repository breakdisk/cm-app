use axum::{routing::get, Json, Router};
use serde_json::{json, Value};

pub fn routes() -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .route("/metrics", get(metrics))
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok", "service": "field-ops" }))
}

async fn ready() -> Json<Value> {
    Json(json!({ "status": "ready" }))
}

async fn metrics() -> String {
    // Prometheus text exposition. Expand as counters are added.
    "# HELP field_ops_up Service liveness\n# TYPE field_ops_up gauge\nfield_ops_up 1\n".to_string()
}
