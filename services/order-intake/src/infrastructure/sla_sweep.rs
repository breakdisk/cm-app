//! Marks shipments past their stored delivery deadline as breached and
//! publishes `SHIPMENT_SLA_BREACHED` for each, so business-logic `SlaBreach`
//! rules can fire. Shipments with no `sla_due_at` (everything created before
//! the column existed) are never touched.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use logisticos_events::{payloads::ShipmentSlaBreached, topics, Event};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::application::services::shipment_service::EventPublisher;
use crate::infrastructure::messaging::status_consumer::TERMINAL_STATUSES;

const BATCH: i64 = 500;

/// One sweep pass. Returns how many breaches were published.
///
/// Claim first (`UPDATE .. RETURNING` under `SKIP LOCKED`, so replicas never
/// double-publish), then publish. A failed publish resets the claim so the next
/// tick retries; a process crash between claim and publish still loses that
/// notice, which is the narrower gap this reset leaves.
pub async fn sweep_once(pool: &PgPool, publisher: &Arc<dyn EventPublisher>) -> anyhow::Result<usize> {
    let terminal: Vec<String> = TERMINAL_STATUSES.iter().map(|s| (*s).to_string()).collect();
    let rows = sqlx::query(
        r#"UPDATE order_intake.shipments
              SET sla_breached_at = now()
            WHERE id IN (
                SELECT id FROM order_intake.shipments
                 WHERE sla_due_at < now()
                   AND sla_breached_at IS NULL
                   AND NOT (status = ANY($1))
                 ORDER BY sla_due_at
                 LIMIT $2
                 FOR UPDATE SKIP LOCKED
            )
        RETURNING id, tenant_id, merchant_id, service_type, sla_due_at, sla_breached_at"#,
    )
    .bind(&terminal)
    .bind(BATCH)
    .fetch_all(pool)
    .await?;

    let mut published = 0usize;
    for r in rows {
        let id: Uuid = r.get("id");
        let tenant_id: Uuid = r.get("tenant_id");
        let due: DateTime<Utc> = r.get("sla_due_at");
        let breached: DateTime<Utc> = r.get("sla_breached_at");
        let event = Event::new(
            "logisticos/order-intake",
            "shipment.sla_breached",
            tenant_id,
            ShipmentSlaBreached {
                shipment_id: id,
                merchant_id: r.get("merchant_id"),
                service_type: r.get("service_type"),
                sla_type: "delivery".into(),
                sla_due_at: due,
                breached_at: breached,
            },
        );
        let sent = match serde_json::to_string(&event) {
            Ok(payload) => publisher
                .publish(topics::SHIPMENT_SLA_BREACHED, &id.to_string(), &payload)
                .await,
            Err(e) => Err(anyhow::anyhow!("serialize: {e}")),
        };
        match sent {
            Ok(()) => published += 1,
            Err(e) => {
                tracing::error!(shipment_id = %id, error = %e, "SLA sweep: publish failed, releasing claim for retry");
                if let Err(e2) = sqlx::query(
                    "UPDATE order_intake.shipments SET sla_breached_at = NULL WHERE id = $1 AND tenant_id = $2",
                )
                .bind(id)
                .bind(tenant_id)
                .execute(pool)
                .await
                {
                    tracing::error!(shipment_id = %id, error = %e2, "SLA sweep: could not release claim; breach notice lost");
                }
            }
        }
    }
    Ok(published)
}

/// Runs `sweep_once` every `secs` seconds, forever.
pub async fn run(pool: PgPool, publisher: Arc<dyn EventPublisher>, secs: u64) {
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(secs.max(1)));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tick.tick().await;
        match sweep_once(&pool, &publisher).await {
            Ok(n) if n > 0 => tracing::info!(count = n, "SLA sweep: published breaches"),
            Ok(_) => {}
            Err(e) => tracing::error!(err = ?e, "SLA sweep failed"),
        }
    }
}
