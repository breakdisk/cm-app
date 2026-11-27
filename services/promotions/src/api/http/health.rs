use axum::{routing::get, Json, Router};
use serde_json::{json, Value};

pub fn routes<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .route("/metrics", get(metrics))
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok", "service": "promotions" }))
}

async fn ready() -> Json<Value> {
    Json(json!({ "status": "ready" }))
}

async fn metrics() -> String {
    "# HELP promotions_up Service liveness\n# TYPE promotions_up gauge\npromotions_up 1\n".to_string()
}
