//! Standardised API response envelope used across all services.
//!
//! Success:  `{ "data": T, "meta": Option<M> }`
//! Error:    `{ "error": { "code": "...", "message": "..." } }`  (handled by AppError::into_response)

use axum::{http::StatusCode, response::IntoResponse, Json};
use serde::Serialize;

/// Wrap a successful payload in the standard envelope and return HTTP 200.
pub fn ok<T: Serialize>(data: T) -> impl IntoResponse {
    (StatusCode::OK, Json(ApiResponse::ok(data)))
}

/// 201 Created with a location body.
pub fn created<T: Serialize>(data: T) -> impl IntoResponse {
    (StatusCode::CREATED, Json(ApiResponse::ok(data)))
}

/// 204 No Content (e.g. after DELETE/revoke).
pub fn no_content() -> impl IntoResponse {
    StatusCode::NO_CONTENT
}

#[derive(Debug, Serialize)]
pub struct ApiResponse<T: Serialize> {
    pub data: T,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<serde_json::Value>,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn ok(data: T) -> Self {
        Self { data, meta: None }
    }

    pub fn with_meta(data: T, meta: serde_json::Value) -> Self {
        Self { data, meta: Some(meta) }
    }
}

/// Paginated response envelope — matches the Rust PaginatedResponse type.
#[derive(Debug, Serialize)]
pub struct PageResponse<T: Serialize> {
    pub data: Vec<T>,
    pub total: u64,
    pub page: u64,
    pub per_page: u64,
    pub total_pages: u64,
}

impl<T: Serialize> PageResponse<T> {
    pub fn new(data: Vec<T>, total: u64, page: u64, per_page: u64) -> Self {
        let total_pages = total.div_ceil(per_page.max(1));
        Self { data, total, page, per_page, total_pages }
    }
}
