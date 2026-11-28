//! Tenant ownership of carrier references.
//!
//! The 3PL adapters hold the platform's credentials, so the carrier will
//! happily cancel, print or track ANY reference it is given, including one
//! another tenant booked. Before a tool hands a `booking_ref` or
//! `tracking_number` to an adapter, it must be found in this tenant's own rows
//! of `carrier.carrier_bookings`, with `tenant_id` from the JWT.
//!
//! A reference that is another tenant's, or unknown, or unverifiable (no
//! booking store configured) all answer with the same "not found", so
//! references cannot be probed across tenants and the check fails closed.

use std::sync::Arc;

use crate::mcp::context::McpContext;
use crate::AppState;

const NOT_FOUND: &str = "No booking with that reference was found for this tenant";

/// Confirms this tenant booked `booking_ref` with `carrier_code`.
pub async fn require_own_booking_ref(
    state: &Arc<AppState>, ctx: &McpContext, carrier_code: &str, booking_ref: &str,
) -> Result<(), String> {
    let Some(store) = &state.booking_repo else {
        tracing::warn!(tenant_id = %ctx.tenant_id, "carrier booking store not configured; refusing booking_ref lookup");
        return Err(NOT_FOUND.into());
    };
    match store.carrier_for_booking_ref(ctx.tenant_id, Some(carrier_code), booking_ref).await {
        Ok(Some(_)) => Ok(()),
        Ok(None) => {
            tracing::warn!(tenant_id = %ctx.tenant_id, actor_uid = %ctx.actor_uid, "booking_ref not owned by tenant");
            Err(NOT_FOUND.into())
        }
        Err(e) => {
            tracing::error!("booking ownership lookup failed: {e}");
            Err("Booking lookup failed; try again".into())
        }
    }
}

/// Confirms this tenant booked `tracking_number` and returns the carrier that
/// issued it. If `carrier_code` is given it must match the booking's carrier.
pub async fn require_own_tracking_number(
    state: &Arc<AppState>, ctx: &McpContext, carrier_code: Option<&str>, tracking_number: &str,
) -> Result<String, String> {
    let Some(store) = &state.booking_repo else {
        tracing::warn!(tenant_id = %ctx.tenant_id, "carrier booking store not configured; refusing tracking lookup");
        return Err(NOT_FOUND.into());
    };
    match store.carrier_for_tracking_number(ctx.tenant_id, carrier_code, tracking_number).await {
        Ok(Some(code)) => Ok(code),
        Ok(None) => {
            tracing::warn!(tenant_id = %ctx.tenant_id, actor_uid = %ctx.actor_uid, "tracking_number not owned by tenant");
            Err(NOT_FOUND.into())
        }
        Err(e) => {
            tracing::error!("tracking ownership lookup failed: {e}");
            Err("Booking lookup failed; try again".into())
        }
    }
}
