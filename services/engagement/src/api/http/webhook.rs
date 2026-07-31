//! Provider webhook handlers for inbound messages and delivery receipts.
//!
//! ## Routes (all unauthenticated — verified by provider signatures)
//!
//! | Method | Path                          | Provider         | Purpose                             |
//! |--------|-------------------------------|------------------|-------------------------------------|
//! | GET    | /v1/webhooks/whatsapp         | Meta             | Webhook verification challenge      |
//! | POST   | /v1/webhooks/whatsapp         | Meta             | Inbound messages + delivery statuses|
//! | POST   | /v1/webhooks/sms/status       | Twilio           | SMS delivery receipt callbacks      |
//! | POST   | /v1/webhooks/email/delivery   | Mailgun/SendGrid | Email open/click/bounce events      |
//!
//! Env vars:
//!   META_WHATSAPP_APP_SECRET    — HMAC-SHA256 body signing secret
//!   META_WHATSAPP_VERIFY_TOKEN  — challenge token registered in Meta App Dashboard
//!   TWILIO_AUTH_TOKEN           — used for Twilio webhook signature validation
//!   EMAIL_WEBHOOK_SECRET        — optional shared secret for email provider webhooks

use std::sync::Arc;
use std::collections::HashMap;

use axum::{
    body::Bytes,
    extract::{Form, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use serde::Deserialize;

use crate::application::services::event_consumer::EngagementPublisher;
use crate::infrastructure::db::{NotificationDb, ReceiptTransition};
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
    /// Twilio Auth Token — used to validate Twilio webhook signatures.
    pub twilio_token: String,
    pub publisher:    Arc<dyn EngagementPublisher>,
    pub db:           Arc<NotificationDb>,
}

// ---------------------------------------------------------------------------
// HMAC-SHA256 signature verification (Meta / general)
// ---------------------------------------------------------------------------

fn verify_hmac_sha256(secret: &str, body: &[u8], signature_header: &str, prefix: &str) -> bool {
    if secret.is_empty() {
        tracing::warn!("Webhook: HMAC verification DISABLED (secret not set)");
        return true;
    }
    let expected_hex = match signature_header.strip_prefix(prefix) {
        Some(h) => h,
        None    => return false,
    };
    let Some(expected_bytes) = hex_decode(expected_hex) else { return false };
    type HmacSha256 = Hmac<Sha256>;
    let Ok(mut mac) = HmacSha256::new_from_slice(secret.as_bytes()) else { return false };
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
// POST /v1/webhooks/whatsapp
// Handles both inbound messages AND delivery/read status updates.
// Meta sends both in the same envelope format; we dispatch on which
// sub-array is present ("messages" vs "statuses").
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

    if !verify_hmac_sha256(&state.app_secret, &body, sig, "sha256=") {
        tracing::warn!("WhatsApp webhook: rejected — invalid Meta signature");
        return (StatusCode::FORBIDDEN, "Invalid signature").into_response();
    }

    let payload: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(v)  => v,
        Err(e) => {
            tracing::warn!(err = %e, "WhatsApp webhook: failed to parse JSON");
            return (StatusCode::BAD_REQUEST, "Invalid JSON").into_response();
        }
    };

    if let Some(entries) = payload["entry"].as_array() {
        for entry in entries {
            let Some(changes) = entry["changes"].as_array() else { continue };
            for change in changes {
                if change["field"].as_str() != Some("messages") { continue; }
                let value        = &change["value"];
                let phone_num_id = value["metadata"]["phone_number_id"].as_str().unwrap_or("");

                // Inbound messages from customers
                if let Some(msgs) = value["messages"].as_array() {
                    for msg in msgs {
                        publish_inbound_event(msg, phone_num_id, value, &state.publisher).await;
                    }
                }

                // Delivery/read status updates for outbound messages
                if let Some(statuses) = value["statuses"].as_array() {
                    for status_obj in statuses {
                        handle_whatsapp_status(status_obj, &state.db).await;
                    }
                }
            }
        }
    }

    // Meta requires 200 OK — any other status triggers a retry storm.
    (StatusCode::OK, "EVENT_RECEIVED").into_response()
}

async fn handle_whatsapp_status(status_obj: &serde_json::Value, db: &NotificationDb) {
    let msg_id = status_obj["id"].as_str().unwrap_or("");
    let status = status_obj["status"].as_str().unwrap_or("");

    // Map Meta status strings to our receipt event names
    let event = match status {
        "sent"      => "sent",
        "delivered" => "delivered",
        "read"      => "read",
        "failed"    => "failed",
        _           => return,
    };

    match db.find_send_id_by_provider_msg_id(msg_id).await {
        Ok(Some(send_id)) => {
            if event == "sent" {
                // Already recorded at dispatch time — skip to avoid clobbering.
                return;
            }
            if let Err(e) = db.apply_receipt(send_id, event, None).await {
                tracing::warn!(msg_id, event, err = %e, "WhatsApp: failed to apply receipt");
            } else {
                tracing::info!(msg_id, event, send_id = %send_id, "WhatsApp delivery receipt applied");
            }
        }
        Ok(None) => {
            tracing::debug!(msg_id, event, "WhatsApp: no send found for provider_message_id");
        }
        Err(e) => {
            tracing::warn!(msg_id, err = %e, "WhatsApp: DB lookup failed for receipt");
        }
    }
}

async fn publish_inbound_event(
    msg:             &serde_json::Value,
    phone_number_id: &str,
    value:           &serde_json::Value,
    publisher:       &Arc<dyn EngagementPublisher>,
) {
    let message_id  = msg["id"].as_str().unwrap_or("");
    let from_phone  = msg["from"].as_str().unwrap_or("");
    let msg_type    = msg["type"].as_str().unwrap_or("text");
    let timestamp   = msg["timestamp"].as_str().unwrap_or("");

    let customer_name = value["contacts"]
        .as_array()
        .and_then(|cs| cs.iter().find(|c| c["wa_id"].as_str() == Some(from_phone)))
        .and_then(|c| c["profile"]["name"].as_str())
        .unwrap_or("");

    let body = match msg_type {
        "text" => msg["text"]["body"].as_str().unwrap_or("").to_owned(),
        other  => format!("[{other} attachment]"),
    };

    let media = if msg_type != "text" {
        let m = &msg[msg_type];
        serde_json::json!([{
            "type":      msg_type,
            "id":        m["id"].as_str().unwrap_or(""),
            "mime_type": m["mime_type"].as_str().unwrap_or(""),
            "caption":   m["caption"].as_str().unwrap_or(""),
        }])
    } else {
        serde_json::json!([])
    };

    tracing::info!(message_id, from = from_phone, msg_type, body_len = body.len(), "WhatsApp inbound");

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

/// Publishes `CAMPAIGN_OPENED`/`CAMPAIGN_CLICKED` for a fresh receipt transition.
/// Drives journey auto-enrollment in the marketing service (journeys whose
/// `trigger.type` is `campaign_opened`).
async fn publish_engagement_transition(t: &ReceiptTransition, publisher: &Arc<dyn EngagementPublisher>) {
    let payload = |event_type: &str| serde_json::json!({
        "campaign_id": t.campaign_id,
        "customer_id": t.customer_id,
        "tenant_id":   t.tenant_id,
        "occurred_at": chrono::Utc::now().to_rfc3339(),
        "event_type":  event_type,
    });

    if t.fresh_open {
        let event = payload("campaign_opened");
        if let Err(e) = publisher.publish(
            topics::CAMPAIGN_OPENED, &t.customer_id.to_string(),
            &serde_json::to_vec(&event).unwrap_or_default(),
        ).await {
            tracing::warn!(campaign_id = %t.campaign_id, customer_id = %t.customer_id, err = %e, "Failed to publish CAMPAIGN_OPENED");
        }
    }
    if t.fresh_click {
        let event = payload("campaign_clicked");
        if let Err(e) = publisher.publish(
            topics::CAMPAIGN_CLICKED, &t.customer_id.to_string(),
            &serde_json::to_vec(&event).unwrap_or_default(),
        ).await {
            tracing::warn!(campaign_id = %t.campaign_id, customer_id = %t.customer_id, err = %e, "Failed to publish CAMPAIGN_CLICKED");
        }
    }
}

// ---------------------------------------------------------------------------
// POST /v1/webhooks/sms/status — Twilio delivery status callback
//
// Twilio posts application/x-www-form-urlencoded with fields:
//   MessageSid, MessageStatus (sent|delivered|undelivered|failed), To, From
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct TwilioStatusForm {
    #[serde(rename = "MessageSid")]
    message_sid: String,
    #[serde(rename = "MessageStatus")]
    message_status: String,
    #[serde(rename = "To")]
    to: Option<String>,
}

async fn handle_sms_status(
    State(state): State<WebhookState>,
    Form(form): Form<TwilioStatusForm>,
) -> Response {
    let event = match form.message_status.as_str() {
        "delivered"   => "delivered",
        "sent"        => return (StatusCode::NO_CONTENT, "").into_response(),
        "undelivered" => "failed",
        "failed"      => "failed",
        other => {
            tracing::debug!(status = other, sid = %form.message_sid, "Twilio: ignoring status");
            return (StatusCode::NO_CONTENT, "").into_response();
        }
    };

    match state.db.find_send_id_by_provider_msg_id(&form.message_sid).await {
        Ok(Some(send_id)) => {
            if let Err(e) = state.db.apply_receipt(send_id, event, None).await {
                tracing::warn!(sid = %form.message_sid, err = %e, "Twilio: failed to apply receipt");
            } else {
                tracing::info!(sid = %form.message_sid, event, "Twilio SMS receipt applied");
            }
        }
        Ok(None) => {
            tracing::debug!(sid = %form.message_sid, "Twilio: no send found for SID");
        }
        Err(e) => {
            tracing::warn!(sid = %form.message_sid, err = %e, "Twilio: DB lookup failed");
        }
    }

    // Twilio retries on non-2xx — always return 204.
    (StatusCode::NO_CONTENT, "").into_response()
}

// ---------------------------------------------------------------------------
// POST /v1/webhooks/email/delivery — email provider events
//
// Accepts a JSON array of events from Mailgun, SendGrid, or compatible
// providers.  Each event object must contain:
//   - `event`   : "delivered" | "opened" | "clicked" | "bounced" | "failed"
//   - `message_id` (or `MessageID` / `sg_message_id`) : provider message ID
//   - `url`     : (optional, "clicked" only) clicked URL
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct EmailEvent {
    // Mailgun uses "message-id" / "Message-Id"; we accept both via aliases.
    #[serde(alias = "MessageID", alias = "sg_message_id", alias = "message-id",
            alias = "Message-Id", rename = "message_id", default)]
    message_id: String,
    event: String,
    url: Option<String>,
}

async fn handle_email_delivery(
    State(state): State<WebhookState>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    // Providers send either a top-level array or `{ "events": [...] }`.
    let events_val = if body.is_array() {
        body.clone()
    } else {
        body.get("events")
            .or_else(|| body.get("event-data").map(|_| &body))
            .cloned()
            .unwrap_or(serde_json::Value::Array(vec![body.clone()]))
    };

    let events: Vec<serde_json::Value> = match events_val.as_array() {
        Some(arr) => arr.clone(),
        None      => vec![events_val],
    };

    for raw in events {
        let ev: EmailEvent = match serde_json::from_value(raw) {
            Ok(e)  => e,
            Err(_) => continue,
        };
        if ev.message_id.is_empty() { continue; }

        let event_name = match ev.event.as_str() {
            "delivered"           => "delivered",
            "opened" | "open"     => "opened",
            "clicked" | "click"   => "clicked",
            "bounced" | "bounce"  => "bounced",
            "failed" | "dropped"  => "failed",
            other => {
                tracing::debug!(event = other, "email webhook: ignoring event");
                continue;
            }
        };

        match state.db.find_send_id_by_provider_msg_id(&ev.message_id).await {
            Ok(Some(send_id)) => {
                let url_ref = ev.url.as_deref();
                match state.db.apply_receipt(send_id, event_name, url_ref).await {
                    Ok(transition) => {
                        tracing::info!(msg_id = %ev.message_id, event = event_name, "email delivery receipt applied");
                        if let Some(t) = transition {
                            publish_engagement_transition(&t, &state.publisher).await;
                        }
                    }
                    Err(e) => tracing::warn!(msg_id = %ev.message_id, err = %e, "email: failed to apply receipt"),
                }
            }
            Ok(None) => {
                tracing::debug!(msg_id = %ev.message_id, "email: no send found for message_id");
            }
            Err(e) => {
                tracing::warn!(msg_id = %ev.message_id, err = %e, "email: DB lookup failed");
            }
        }
    }

    (StatusCode::OK, "OK").into_response()
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn webhook_router(state: WebhookState) -> Router {
    Router::new()
        .route("/v1/webhooks/whatsapp",       get(verify_webhook).post(handle_whatsapp_inbound))
        .route("/v1/webhooks/sms/status",     post(handle_sms_status))
        .route("/v1/webhooks/email/delivery", post(handle_email_delivery))
        .with_state(state)
}
