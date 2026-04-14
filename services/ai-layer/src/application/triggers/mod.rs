/// Kafka-driven agent triggers — listens to domain events and spawns agents.
///
/// This is the autonomous activation layer. When a domain event arrives,
/// the trigger evaluates policy to decide which (if any) agent to spawn.
///
/// Current trigger policies:
///   shipment.created     → DispatchAgent  (auto-assign driver)
///   delivery.failed      → RecoveryAgent  (reschedule + notify)
///   cod.collected        → ReconciliationAgent (verify wallet credit within 5 min)
///   analytics.anomaly    → AnomalyAgent   (alert ops)
use std::sync::Arc;
use rdkafka::{
    consumer::{CommitMode, Consumer, StreamConsumer},
    message::BorrowedMessage,
    Message,
};
use serde_json::Value;
use uuid::Uuid;

use logisticos_events::topics;
use logisticos_types::TenantId;

use crate::{
    application::agent::AgentRunner,
    domain::entities::AgentType,
};

pub async fn run_trigger_consumer(
    consumer: Arc<StreamConsumer>,
    runner:   Arc<AgentRunner>,
) {
    consumer
        .subscribe(&[
            topics::SHIPMENT_CREATED,
            topics::DELIVERY_FAILED,
            topics::COD_COLLECTED,
        ])
        .expect("AI trigger consumer subscription failed");

    loop {
        match consumer.recv().await {
            Ok(msg) => {
                if let Err(e) = handle_trigger(&msg, &runner).await {
                    tracing::warn!(
                        topic = msg.topic(),
                        err = %e,
                        "Agent trigger error"
                    );
                }
                consumer.commit_message(&msg, CommitMode::Async).ok();
            }
            Err(e) => {
                tracing::error!(err = %e, "AI trigger Kafka recv error");
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        }
    }
}

async fn handle_trigger(
    msg: &BorrowedMessage<'_>,
    runner: &Arc<AgentRunner>,
) -> anyhow::Result<()> {
    let payload = match msg.payload() {
        Some(p) => serde_json::from_slice::<Value>(p)?,
        None => return Ok(()),
    };

    let tenant_id = extract_tenant_id(msg)?;

    match msg.topic() {
        topics::SHIPMENT_CREATED => {
            let shipment_id = payload["shipment_id"].as_str().unwrap_or("unknown");
            let origin      = payload["origin_address"].as_str().unwrap_or("unknown");

            let user_msg = format!(
                "A new shipment has been created: shipment_id={}, origin_address='{}'. \
                 Find the best available driver and assign them. \
                 If no drivers are available within 10km, escalate to a human.",
                shipment_id, origin
            );

            let runner = runner.clone();
            let tenant = tenant_id.clone();
            let trig   = payload.clone();
            tokio::spawn(async move {
                match runner.run(tenant, AgentType::Dispatch, trig, user_msg).await {
                    Ok(s) => tracing::info!(session_id = %s.id, status = ?s.status, "Dispatch agent completed"),
                    Err(e) => tracing::error!(err = %e, "Dispatch agent failed"),
                }
            });
        }

        topics::DELIVERY_FAILED => {
            let shipment_id    = payload["shipment_id"].as_str().unwrap_or("unknown");
            let reason         = payload["reason"].as_str().unwrap_or("unknown reason");
            let attempt_number = payload["attempt_number"].as_u64().unwrap_or(1);

            let user_msg = format!(
                "Delivery failed for shipment_id={}. Reason: '{}'. This is attempt #{}. \
                 Please: 1) Reschedule the delivery to the next available slot, \
                 2) Send a customer notification with the updated ETA. \
                 If this is attempt 3 or more, escalate to a human operator.",
                shipment_id, reason, attempt_number
            );

            let runner = runner.clone();
            let tenant = tenant_id.clone();
            let trig   = payload.clone();
            tokio::spawn(async move {
                match runner.run(tenant, AgentType::Recovery, trig, user_msg).await {
                    Ok(s) => tracing::info!(session_id = %s.id, status = ?s.status, "Recovery agent completed"),
                    Err(e) => tracing::error!(err = %e, "Recovery agent failed"),
                }
            });
        }

        topics::COD_COLLECTED => {
            let shipment_id  = payload["shipment_id"].as_str().unwrap_or("unknown");
            let amount_cents = payload["amount_cents"].as_i64().unwrap_or(0);

            let user_msg = format!(
                "COD of {} centavos was collected for shipment_id={}. \
                 Verify that the merchant wallet has been credited net of the 1.5% platform fee. \
                 If the credit has not been applied within the last 10 minutes, trigger reconciliation.",
                amount_cents, shipment_id
            );

            let runner = runner.clone();
            let tenant = tenant_id.clone();
            let trig   = payload.clone();
            tokio::spawn(async move {
                match runner.run(tenant, AgentType::Reconciliation, trig, user_msg).await {
                    Ok(s) => tracing::info!(session_id = %s.id, status = ?s.status, "Reconciliation agent completed"),
                    Err(e) => tracing::error!(err = %e, "Reconciliation agent failed"),
                }
            });
        }

        _ => {}
    }

    Ok(())
}

fn extract_tenant_id(msg: &BorrowedMessage<'_>) -> anyhow::Result<TenantId> {
    use rdkafka::message::Headers;
    msg.headers()
        .and_then(|h| {
            h.iter().find_map(|header| {
                if header.key == "tenant_id" {
                    header.value
                        .and_then(|v| std::str::from_utf8(v).ok())
                        .and_then(|s: &str| s.parse::<Uuid>().ok())
                        .map(TenantId::from_uuid)
                } else { None }
            })
        })
        .ok_or_else(|| anyhow::anyhow!("Missing tenant_id header on topic {}", msg.topic()))
}
