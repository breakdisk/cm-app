use axum::{http::StatusCode, response::{IntoResponse, Response}, Json};
use serde_json::json;
use thiserror::Error;

/// Canonical application error type used across all services.
#[derive(Debug, Error)]
pub enum AppError {
    // ── Auth ────────────────────────────────────────────────
    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Forbidden: insufficient permissions for {resource}")]
    Forbidden { resource: String },

    // ── Validation ──────────────────────────────────────────
    #[error("Validation failed: {0}")]
    Validation(String),

    #[error("Not found: {resource} with id {id}")]
    NotFound { resource: &'static str, id: String },

    #[error("Conflict: {0}")]
    Conflict(String),

    // ── Business Rules ──────────────────────────────────────
    #[error("Business rule violation: {0}")]
    BusinessRule(String),

    #[error("Subscription tier {tier} does not allow {feature}")]
    SubscriptionLimitExceeded { tier: String, feature: String },

    #[error("SLA breach: {0}")]
    SlaBreach(String),

    // ── External ────────────────────────────────────────────
    #[error("External service error: {service} — {message}")]
    ExternalService { service: String, message: String },

    #[error("Payment processing failed: {0}")]
    PaymentFailed(String),

    // ── Infrastructure ──────────────────────────────────────
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Cache error: {0}")]
    Cache(String),

    #[error("Event publishing failed: {0}")]
    EventPublish(String),

    // ── Generic ─────────────────────────────────────────────
    #[error("Internal server error")]
    Internal(#[from] anyhow::Error),
}

impl AppError {
    /// Convenience constructor for use as a function pointer in `.map_err(AppError::internal)`.
    pub fn internal(e: anyhow::Error) -> Self {
        AppError::Internal(e)
    }

    pub fn status_code(&self) -> StatusCode {
        match self {
            AppError::Unauthorized(_)              => StatusCode::UNAUTHORIZED,
            AppError::Forbidden { .. }             => StatusCode::FORBIDDEN,
            AppError::Validation(_)                => StatusCode::UNPROCESSABLE_ENTITY,
            AppError::NotFound { .. }              => StatusCode::NOT_FOUND,
            AppError::Conflict(_)                  => StatusCode::CONFLICT,
            AppError::BusinessRule(_)              => StatusCode::UNPROCESSABLE_ENTITY,
            AppError::SubscriptionLimitExceeded {..} => StatusCode::PAYMENT_REQUIRED,
            AppError::SlaBreach(_)                 => StatusCode::UNPROCESSABLE_ENTITY,
            AppError::PaymentFailed(_)             => StatusCode::PAYMENT_REQUIRED,
            _                                      => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn error_code(&self) -> &'static str {
        match self {
            AppError::Unauthorized(_)              => "UNAUTHORIZED",
            AppError::Forbidden { .. }             => "FORBIDDEN",
            AppError::Validation(_)                => "VALIDATION_ERROR",
            AppError::NotFound { .. }              => "NOT_FOUND",
            AppError::Conflict(_)                  => "CONFLICT",
            AppError::BusinessRule(_)              => "BUSINESS_RULE_VIOLATION",
            AppError::SubscriptionLimitExceeded {..} => "SUBSCRIPTION_LIMIT_EXCEEDED",
            AppError::SlaBreach(_)                 => "SLA_BREACH",
            AppError::PaymentFailed(_)             => "PAYMENT_FAILED",
            AppError::ExternalService { .. }       => "EXTERNAL_SERVICE_ERROR",
            AppError::Database(_)                  => "DATABASE_ERROR",
            AppError::Cache(_)                     => "CACHE_ERROR",
            AppError::EventPublish(_)              => "EVENT_PUBLISH_ERROR",
            AppError::Internal(_)                  => "INTERNAL_SERVER_ERROR",
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let body = json!({
            "error": {
                "code": self.error_code(),
                "message": self.to_string(),
            }
        });
        (status, Json(body)).into_response()
    }
}

pub type AppResult<T> = Result<T, AppError>;
