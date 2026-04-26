//! Webhook dispatcher.
//!
//! Subscribes to a curated set of platform Kafka topics, looks up which
//! tenant webhooks subscribe to each event, and POSTs the payload to the
//! webhook URL with an HMAC-SHA256 signature header.
//!
//! Failed deliveries are retried with exponential backoff up to
//! MAX_RETRY_ATTEMPTS. Every attempt — success or failure — is recorded
//! in `webhooks.deliveries` so admins can audit what their endpoint actually
//! received and replied with.

use std::sync::Arc;
use std::time::Duration;

use hmac::{Hmac, Mac};
use rdkafka::{
    consumer::{CommitMode, Consumer, StreamConsumer},
    config::ClientConfig,
    Message,
};
use sha2::Sha256;
use tokio::sync::watch;
use uuid::Uuid;

use crate::domain::{
    entities::DeliveryAttempt,
    repositories::{DeliveryRepository, WebhookRepository},
};

/// Topics the dispatcher subscribes to. Any new topic added here can be
/// referenced by tenant webhook subscriptions via `events: ["shipment.created"]`
/// (the topic suffix after `logisticos.`). The wildcard `"*"` subscription
/// receives every event from every topic in this list.
const SUBSCRIBED_TOPICS: &[&str] = &[
    "logisticos.identity.tenant.created",
    "logisticos.identity.user.created",
    "logisticos.order.shipment.created",
    "logisticos.order.shipment.confirmed",
    "logisticos.order.shipment.cancelled",
    "logisticos.dispatch.driver.assigned",
    "logisticos.driver.pickup.completed",
    "logisticos.driver.delivery.completed",
    "logisticos.driver.delivery.failed",
    "logisticos.payments.invoice.finalized",
    "logisticos.payments.cod.remittance_ready",
];

const MAX_RETRY_ATTEMPTS: u32 = 3;
const REQUEST_TIMEOUT_SECS: u64 = 10;

/// Wire-shape of every Kafka message the platform produces. Mirrors
/// `logisticos_events::envelope::Event<T>` minus the typed payload — the
/// dispatcher works with raw JSON so it doesn't need a per-payload type.
#[derive(serde::Deserialize)]
struct EventEnvelope {
    #[serde(default)]
    id:         Option<Uuid>,
    #[serde(default)]
    source:     Option<String>,
    event_type: String,
    tenant_id:  Uuid,
    #[serde(default)]
    time:       Option<String>,
    data:       serde_json::Value,
}

pub async fn start(
    brokers:    &str,
    group_id:   &str,
    webhook_repo:  Arc<dyn WebhookRepository>,
    delivery_repo: Arc<dyn DeliveryRepository>,
    mut shutdown:  watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", brokers)
        .set("group.id", &format!("{group_id}-dispatcher"))
        .set("auto.offset.reset", "latest") // skip historical events on first start
        .set("enable.auto.commit", "false")
        .create()?;

    consumer.subscribe(SUBSCRIBED_TOPICS)?;
    tracing::info!(topics = ?SUBSCRIBED_TOPICS, "webhook dispatcher subscribed");

    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .build()?;

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    tracing::info!("webhook dispatcher: shutdown signal received");
                    break;
                }
            }
            result = consumer.recv() => {
                match result {
                    Ok(msg) => {
                        if let Some(payload) = msg.payload() {
                            if let Err(e) = handle(
                                payload,
                                webhook_repo.as_ref(),
                                delivery_repo.as_ref(),
                                &http,
                            ).await {
                                tracing::warn!(err = %e, "webhook dispatcher: handler error (skipping)");
                            }
                        }
                        let _ = consumer.commit_message(&msg, CommitMode::Async);
                    }
                    Err(e) => {
                        tracing::error!(err = %e, "webhook dispatcher: recv error");
                        tokio::time::sleep(Duration::from_millis(500)).await;
                    }
                }
            }
        }
    }
    Ok(())
}

async fn handle(
    payload: &[u8],
    webhook_repo:  &dyn WebhookRepository,
    delivery_repo: &dyn DeliveryRepository,
    http: &reqwest::Client,
) -> anyhow::Result<()> {
    let envelope: EventEnvelope = serde_json::from_slice(payload)?;
    let subscribers = webhook_repo
        .find_subscribers(envelope.tenant_id, &envelope.event_type)
        .await?;

    if subscribers.is_empty() {
        return Ok(());
    }

    // Re-serialize the full envelope (we strip nothing — receivers may
    // care about source/time/id for idempotency).
    let body = serde_json::to_vec(&serde_json::json!({
        "id":         envelope.id,
        "source":     envelope.source,
        "event_type": envelope.event_type,
        "tenant_id":  envelope.tenant_id,
        "time":       envelope.time,
        "data":       envelope.data,
    }))?;

    for w in subscribers {
        deliver_with_retry(http, webhook_repo, delivery_repo, &w.url, &w.id, &w.tenant_id, &w.secret, &envelope.event_type, &envelope.data, &body).await;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn deliver_with_retry(
    http: &reqwest::Client,
    webhook_repo:  &dyn WebhookRepository,
    delivery_repo: &dyn DeliveryRepository,
    url: &str,
    webhook_id: &Uuid,
    tenant_id:  &Uuid,
    secret:     &str,
    event_type: &str,
    payload_data: &serde_json::Value,
    body: &[u8],
) {
    let signature = sign_hmac_sha256(secret.as_bytes(), body);

    for attempt in 0..MAX_RETRY_ATTEMPTS {
        let started = std::time::Instant::now();
        let req = http
            .post(url)
            .header("content-type", "application/json")
            .header("x-logisticos-event", event_type)
            .header("x-logisticos-signature", format!("sha256={signature}"))
            .header("x-logisticos-delivery", Uuid::new_v4().to_string())
            .header("x-logisticos-attempt",  attempt.to_string())
            .body(body.to_vec())
            .send()
            .await;
        let elapsed_ms = started.elapsed().as_millis() as i32;

        let (status_code, body_str) = match req {
            Ok(resp) => {
                let code = resp.status().as_u16() as i32;
                // Cap response body so a chatty receiver can't blow up our DB.
                let text = resp.text().await.unwrap_or_default();
                (code, Some(if text.len() > 4096 { text[..4096].to_string() } else { text }))
            }
            Err(e) => (0_i32, Some(format!("transport error: {e}"))),
        };

        let success = (200..300).contains(&status_code);

        // Record this attempt regardless of success.
        let attempt_row = DeliveryAttempt {
            id:            Uuid::new_v4(),
            webhook_id:    *webhook_id,
            tenant_id:     *tenant_id,
            event_type:    event_type.to_owned(),
            payload:       payload_data.clone(),
            attempt:       attempt as i32,
            status_code,
            response_body: body_str,
            duration_ms:   elapsed_ms,
            delivered_at:  chrono::Utc::now(),
        };
        if let Err(e) = delivery_repo.save(&attempt_row).await {
            tracing::warn!(webhook_id = %webhook_id, err = %e, "failed to persist delivery attempt");
        }
        let _ = webhook_repo.record_attempt(*webhook_id, success, status_code).await;

        if success {
            return;
        }

        // Exponential backoff: 1s, 2s, 4s. Skip sleep on the final attempt.
        if attempt + 1 < MAX_RETRY_ATTEMPTS {
            tokio::time::sleep(Duration::from_secs(1u64 << attempt)).await;
        }
    }

    tracing::warn!(webhook_id = %webhook_id, event_type = %event_type, "webhook delivery exhausted retries");
}

fn sign_hmac_sha256(key: &[u8], body: &[u8]) -> String {
    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(body);
    hex::encode(mac.finalize().into_bytes())
}
