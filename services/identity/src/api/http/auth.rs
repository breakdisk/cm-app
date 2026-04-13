use axum::{extract::State, Json};
use std::sync::Arc;
use crate::{
    api::http::AppState,
    application::commands::{LoginCommand, RefreshTokenCommand, ForgotPasswordCommand, ResetPasswordCommand, RegisterCommand, SendVerificationEmailCommand, VerifyEmailCommand, OtpSendCommand, OtpVerifyCommand},
};
use logisticos_errors::AppError;

pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(cmd): Json<LoginCommand>,
) -> Result<Json<serde_json::Value>, AppError> {
    let result = state.auth_service.login(cmd).await?;
    Ok(Json(serde_json::json!({ "data": result })))
}

pub async fn refresh(
    State(state): State<Arc<AppState>>,
    Json(cmd): Json<RefreshTokenCommand>,
) -> Result<Json<serde_json::Value>, AppError> {
    let result = state.auth_service.refresh(cmd).await?;
    Ok(Json(serde_json::json!({ "data": result })))
}

pub async fn forgot_password(
    State(state): State<Arc<AppState>>,
    Json(cmd): Json<ForgotPasswordCommand>,
) -> Result<Json<serde_json::Value>, AppError> {
    state.auth_service.forgot_password(cmd).await?;
    Ok(Json(serde_json::json!({ "data": { "message": "If that email exists, a reset link has been sent." } })))
}

pub async fn reset_password(
    State(state): State<Arc<AppState>>,
    Json(cmd): Json<ResetPasswordCommand>,
) -> Result<Json<serde_json::Value>, AppError> {
    state.auth_service.reset_password(cmd).await?;
    Ok(Json(serde_json::json!({ "data": { "message": "Password reset successfully." } })))
}

pub async fn send_verification_email(
    State(state): State<Arc<AppState>>,
    Json(cmd): Json<SendVerificationEmailCommand>,
) -> Result<Json<serde_json::Value>, AppError> {
    state.auth_service.send_verification_email(cmd).await?;
    Ok(Json(serde_json::json!({ "data": { "message": "Verification email sent if account exists." } })))
}

pub async fn verify_email(
    State(state): State<Arc<AppState>>,
    Json(cmd): Json<VerifyEmailCommand>,
) -> Result<Json<serde_json::Value>, AppError> {
    state.auth_service.verify_email(cmd).await?;
    Ok(Json(serde_json::json!({ "data": { "message": "Email verified successfully." } })))
}

pub async fn register(
    State(state): State<Arc<AppState>>,
    Json(cmd): Json<RegisterCommand>,
) -> Result<Json<serde_json::Value>, AppError> {
    state.auth_service.register(cmd).await?;
    Ok(Json(serde_json::json!({
        "data": { "message": "Registration successful. Please verify your email." }
    })))
}

// ─── OTP endpoints (driver app + customer app) ──────────────────────────────

pub async fn send_otp(
    State(state): State<Arc<AppState>>,
    Json(cmd): Json<OtpSendCommand>,
) -> Result<Json<serde_json::Value>, AppError> {
    state.auth_service.otp_send(cmd).await?;
    Ok(Json(serde_json::json!({ "data": { "message": "OTP sent." } })))
}

pub async fn verify_otp(
    State(state): State<Arc<AppState>>,
    Json(cmd): Json<OtpVerifyCommand>,
) -> Result<Json<serde_json::Value>, AppError> {
    let result = state.auth_service.otp_verify(cmd).await?;
    Ok(Json(serde_json::json!({ "data": result })))
}
