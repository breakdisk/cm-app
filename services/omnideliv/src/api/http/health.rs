use axum::{routing::get, Json, Router};
use serde_json::{json, Value};

pub fn routes() -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .route("/metrics", get(metrics))
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok", "service": "omnideliv" }))
}

async fn ready() -> Json<Value> {
    Json(json!({ "status": "ready" }))
}

async fn metrics() -> String {
    "# HELP omnideliv_up Service liveness\n# TYPE omnideliv_up gauge\nomnideliv_up 1\n".to_string()
}
