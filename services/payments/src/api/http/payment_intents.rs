//! POST /v1/internal/payments/intents — mesh-internal only (Istio mTLS gates
//! caller identity, same as every other route under /v1/internal). Callable
//! by order-intake to create a payment session for an amount order-intake has
//! already priced and verified — payments trusts the caller's amount here
//! specifically because this route is unreachable from any tenant-facing
//! credential, per the design spec's D3.

use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use logisticos_errors::AppError;

use crate::api::http::AppState;
use crate::application::services::payment_intent_service::CreateIntentCommand;

#[derive(Deserialize)]
pub struct CreateIntentRequest {
    pub tenant_id: Uuid,
    pub purpose: String,
    pub reference_type: String,
    pub reference_id: Uuid,
    pub amount_cents: i64,
    pub currency: String,
    pub return_url: String,
}

#[derive(Serialize)]
pub struct CreateIntentResponse {
    pub intent_id: Uuid,
    pub checkout_url: String,
}

pub async fn create_intent(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateIntentRequest>,
) -> Result<(StatusCode, Json<CreateIntentResponse>), AppError> {
    let created = state.payment_intent_service.create_intent(CreateIntentCommand {
        tenant_id: req.tenant_id,
        purpose: req.purpose,
        reference_type: req.reference_type,
        reference_id: req.reference_id,
        amount_cents: req.amount_cents,
        currency: req.currency,
        return_url: req.return_url,
    }).await?;

    Ok((
        StatusCode::CREATED,
        Json(CreateIntentResponse { intent_id: created.intent_id, checkout_url: created.checkout_url }),
    ))
}
