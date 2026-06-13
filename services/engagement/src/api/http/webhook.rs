//! Inbound WhatsApp webhook — receives Meta Cloud API callbacks when a customer
//! messages the business WhatsApp number.
//!
//! Two routes (both at /v1/webhooks/whatsapp, unauthenticated):
//!
//!   GET  — Meta webhook verification challenge.  Meta sends this once when
//!           the webhook URL is first registered in the App Dashboard.
//!
//!   POST — Inbound message delivery.  Meta posts a JSON envelope; we verify
//!           the HMAC-SHA256 signature, extract each message, and publish a
//!           `logisticos.engagement.whatsapp.inbound` Kafka event per message
//!           for the AI layer to process.
//!
//! Env vars:
//!   META_WHATSAPP_APP_SECRET    — used for HMAC-SHA256 request verification
//!   META_WHATSAPP_VERIFY_TOKEN  — must match the token configured in Meta App Dashboard

use std::sync::Arc;
use std::collections::HashMap;

use axum::{
    body::Bytes,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::application::services::event_consumer::EngagementPublisher;
use logisticos_events::topics;

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct WebhookState {
    /// Meta App Secret for HMAC-SHA256 signature verification.
    /// Empty = skip verification (dev/test mode; warns loudly).
    pub app_secret:   String,
    /// Token configured in Meta App Dashboard → Webhooks → Verify Token.
    pub verify_token: String,
    pub publisher:    Arc<dyn EngagementPublisher>,
}

// ---------------------------------------------------------------------------
// HMAC-SHA256 signature verification
// ---------------------------------------------------------------------------

/// Verifies `X-Hub-Signature-256` header produced by Meta.
///
/// Header format: `sha256=<lowercase-hex>`
/// Returns `true` when `app_secret` is empty (dev bypass — logged loudly).
fn verify_signature(app_secret: &str, body: &[u8], signature_header: &str) -> bool {
    if app_secret.is_empty() {
        tracing::warn!("WhatsApp webhook: signature verification DISABLED (META_WHATSAPP_APP_SECRET not set)");
        return true;
    }

    let expected_hex = match signature_header.strip_prefix("sha256=") {
        Some(h) => h,
        None    => return false,
    };

    let Some(expected_bytes) = hex_decode(expected_hex) else {
        return false;
    };

    type HmacSha256 = Hmac<Sha256>;
    let Ok(mut mac) = HmacSha256::new_from_slice(app_secret.as_bytes()) else {
        return false;
    };
    mac.update(body);
    mac.verify_slice(&expected_bytes).is_ok()
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 { return None; }
    s.as_bytes()
        .chunks(2)
        .map(|pair| {
            let hi = hex_nibble(pair[0])?;
            let lo = hex_nibble(pair[1])?;
            Some((hi << 4) | lo)
        })
        .collect()
}

fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _           => None,
    }
}

// ---------------------------------------------------------------------------
// GET /v1/webhooks/whatsapp — Meta verification challenge
// ---------------------------------------------------------------------------

async fn verify_webhook(
    State(state): State<WebhookState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let mode      = params.get("hub.mode").map(|s| s.as_str()).unwrap_or("");
    let challenge = params.get("hub.challenge").map(|s| s.as_str()).unwrap_or("");
    let token     = params.get("hub.verify_token").map(|s| s.as_str()).unwrap_or("");

    if mode == "subscribe" && !state.verify_token.is_empty() && token == state.verify_token {
        tracing::info!("WhatsApp webhook verification successful");
        (StatusCode::OK, challenge.to_owned()).into_response()
    } else {
        tracing::warn!(
            mode,
            token_match = (token == state.verify_token),
            "WhatsApp webhook verification failed"
        );
        (StatusCode::FORBIDDEN, "Verification failed").into_response()
    }
}

// ---------------------------------------------------------------------------
// POST /v1/webhooks/whatsapp — inbound message delivery
// ---------------------------------------------------------------------------

async fn handle_whatsapp_inbound(
    State(state): State<WebhookState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let sig = headers
        .get("x-hub-signature-256")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if !verify_signature(&state.app_secret, &body, sig) {
        tracing::warn!("WhatsApp inbound: rejected — invalid Meta signature");
        return (StatusCode::FORBIDDEN, "Invalid signature").into_response();
    }

    let payload: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(v)  => v,
        Err(e) => {
            tracing::warn!(err = %e, "WhatsApp inbound: failed to parse JSON body");
            return (StatusCode::BAD_REQUEST, "Invalid JSON").into_response();
        }
    };

    // Meta envelope: { "object": "whatsapp_business_account", "entry": [...] }
    if let Some(entries) = payload["entry"].as_array() {
        for entry in entries {
            let Some(changes) = entry["changes"].as_array() else { continue };
            for change in changes {
                if change["field"].as_str() != Some("messages") { continue; }
                let value        = &change["value"];
                let phone_num_id = value["metadata"]["phone_number_id"].as_str().unwrap_or("");
                let Some(msgs)   = value["messages"].as_array() else { continue };

                for msg in msgs {
                    publish_inbound_event(msg, phone_num_id, value, &state.publisher).await;
                }
            }
        }
    }

    // Meta requires 200 OK — any other status triggers a retry storm.
    (StatusCode::OK, "EVENT_RECEIVED").into_response()
}

async fn publish_inbound_event(
    msg:             &serde_json::Value,
    phone_number_id: &str,
    value:           &serde_json::Value,
    publisher:       &Arc<dyn EngagementPublisher>,
) {
    let message_id  = msg["id"].as_str().unwrap_or("");
    let from_phone  = msg["from"].as_str().unwrap_or(""); // E.164 without +, e.g. "639171234567"
    let msg_type    = msg["type"].as_str().unwrap_or("text");
    let timestamp   = msg["timestamp"].as_str().unwrap_or("");

    let customer_name = value["contacts"]
        .as_array()
        .and_then(|cs| cs.iter().find(|c| c["wa_id"].as_str() == Some(from_phone)))
        .and_then(|c| c["profile"]["name"].as_str())
        .unwrap_or("");

    let body = match msg_type {
        "text"     => msg["text"]["body"].as_str().unwrap_or("").to_owned(),
        other      => format!("[{other} attachment]"),
    };

    let media = if msg_type != "text" {
        let m = &msg[msg_type];
        serde_json::json!([{
            "type":         msg_type,
            "id":           m["id"].as_str().unwrap_or(""),
            "mime_type":    m["mime_type"].as_str().unwrap_or(""),
            "caption":      m["caption"].as_str().unwrap_or(""),
        }])
    } else {
        serde_json::json!([])
    };

    tracing::info!(
        message_id,
        from = from_phone,
        msg_type,
        body_len = body.len(),
        "WhatsApp inbound message received"
    );

    let event = serde_json::json!({
        "message_id":         message_id,
        "from_phone":         from_phone,
        "to_phone_number_id": phone_number_id,
        "customer_name":      customer_name,
        "body":               body,
        "message_type":       msg_type,
        "media":              media,
        "timestamp":          timestamp,
        "received_at":        chrono::Utc::now().to_rfc3339(),
    });

    match publisher.publish(
        topics::WHATSAPP_INBOUND,
        from_phone,
        &serde_json::to_vec(&event).unwrap_or_default(),
    ).await {
        Ok(_)  => tracing::info!(message_id, "WhatsApp inbound event published"),
        Err(e) => tracing::error!(message_id, err = %e, "Failed to publish WhatsApp inbound event"),
    }
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn webhook_router(state: WebhookState) -> Router {
    Router::new()
        .route("/v1/webhooks/whatsapp", get(verify_webhook).post(handle_whatsapp_inbound))
        .with_state(state)
}
