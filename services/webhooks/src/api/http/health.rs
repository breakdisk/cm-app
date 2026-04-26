use axum::http::StatusCode;

pub async fn health() -> StatusCode { StatusCode::OK }
pub async fn ready()  -> StatusCode { StatusCode::OK }
