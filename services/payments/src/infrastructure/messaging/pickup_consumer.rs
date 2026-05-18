//! Kafka consumer for `logisticos.pod.pickup.captured`.
//!
//! # What this consumer does
//!
//! On every `pickup.captured` event:
//!
//! 1. **Track A (Balikbayan)** — `service_code == "balikbayan"` AND `declared_value_cents > 0`:
//!    Opens (or re-uses) the driver's active ledger for this shift, debits the
//!    declared value, and saves. This records the driver's cash liability before
//!    they depart the merchant with the parcel.
//!
//! 2. **All other service codes** (standard, express, …):
//!    No-op — weight-based surcharges are handled by the hub weigh-in consumer
//!    once the parcel is scanned at the hub.
//!
//! # Idempotency
//!
//! The ledger's `ON CONFLICT (id) DO NOTHING` on entries means replaying the same
//! Kafka event is always safe — the debit will already exist for `pop_id`, so the
//! entry insert does nothing and the balance remains correct.
//!
//! The top-level guard checks whether an entry for `pop_id` already exists before
//! calling `debit_pickup` to avoid double-incrementing `balance_cents`.

use std::sync::Arc;
use anyhow::Context;
use logisticos_events::{
    consumer::KafkaConsumer,
    payloads::PickupCaptured,
    topics::PICKUP_CAPTURED,
};
use logisticos_types::TenantId;

use crate::domain::repositories::DriverLedgerRepository;

pub struct PickupCapturedConsumer {
    inner:          KafkaConsumer,
    ledger_repo:    Arc<dyn DriverLedgerRepository>,
}

impl PickupCapturedConsumer {
    pub fn new(
        brokers:      &str,
        group_id:     &str,
        ledger_repo:  Arc<dyn DriverLedgerRepository>,
    ) -> anyhow::Result<Self> {
        let inner = KafkaConsumer::new(brokers, group_id, &[PICKUP_CAPTURED])
            .context("Failed to create PickupCapturedConsumer Kafka client")?;
        Ok(Self { inner, ledger_repo })
    }

    pub async fn run(self) {
        let ledger_repo = self.ledger_repo;

        let result = self.inner.run(move |_topic, json| {
            let repo = Arc::clone(&ledger_repo);
            async move { handle_pickup_captured(repo, json).await }
        }).await;

        if let Err(e) = result {
            tracing::error!("PickupCapturedConsumer loop exited with error: {e}");
        }
    }
}

async fn handle_pickup_captured(
    ledger_repo: Arc<dyn DriverLedgerRepository>,
    json:        serde_json::Value,
) -> anyhow::Result<()> {
    let payload: PickupCaptured = serde_json::from_value(
        json.get("data").cloned().unwrap_or(json.clone()),
    )
    .context("Failed to deserialise PickupCaptured payload")?;

    // ── Only Track A (Balikbayan) triggers a driver ledger debit ──────────────
    // Standard / express pickups are weight-billed at hub — no pre-debit needed.
    let is_track_a = payload.service_code.to_lowercase() == "balikbayan";
    let debit_cents = payload.declared_value_cents.unwrap_or(0);

    if !is_track_a || debit_cents <= 0 {
        tracing::debug!(
            pop_id       = %payload.pop_id,
            service_code = %payload.service_code,
            "pickup.captured — non-Track-A or zero value; skipping ledger debit"
        );
        return Ok(());
    }

    let tenant_id = TenantId::from_uuid(payload.tenant_id);

    tracing::info!(
        pop_id       = %payload.pop_id,
        shipment_id  = %payload.shipment_id,
        driver_id    = %payload.driver_id,
        debit_cents  = debit_cents,
        service_code = %payload.service_code,
        "Track A pickup — debiting driver ledger"
    );

    // Find or create the driver's open ledger for this shift.
    // `shift_id` is not carried in PickupCaptured today (no shift management yet);
    // pass `None` so a single open ledger accumulates all Track A pickups.
    let mut ledger = ledger_repo
        .find_or_create_for_shift(&tenant_id, payload.driver_id, None)
        .await
        .context("Failed to find/create driver ledger")?;

    // Idempotency guard — check whether this pop_id already has an entry.
    let already_debited = ledger.entries.iter().any(|e| {
        e.reference.as_deref() == Some(&payload.pop_id.to_string())
            && matches!(e.kind, crate::domain::entities::EntryKind::PickupDebit)
    });

    if already_debited {
        tracing::info!(
            pop_id = %payload.pop_id,
            "Track A debit already recorded for this pop_id — idempotent skip"
        );
        return Ok(());
    }

    ledger.debit_pickup(debit_cents, payload.shipment_id, payload.pop_id);

    // Retry once on optimistic-lock conflict — a concurrent event on the same
    // driver may have raced the version. One reload-and-retry is sufficient.
    match ledger_repo.save(&ledger).await {
        Ok(()) => {
            tracing::info!(
                pop_id      = %payload.pop_id,
                driver_id   = %payload.driver_id,
                ledger_id   = %ledger.id,
                balance     = ledger.balance_cents,
                "Driver ledger debited for Track A pickup"
            );
        }
        Err(e) if e.to_string().contains("Optimistic lock conflict") => {
            tracing::warn!(
                pop_id    = %payload.pop_id,
                ledger_id = %ledger.id,
                "Optimistic lock conflict — reloading ledger and retrying"
            );
            // Reload, re-apply, re-save.
            let mut fresh = ledger_repo
                .find_or_create_for_shift(&tenant_id, payload.driver_id, None)
                .await
                .context("Failed to reload driver ledger on retry")?;

            // Only debit if still not present after reload.
            let still_absent = !fresh.entries.iter().any(|e| {
                e.reference.as_deref() == Some(&payload.pop_id.to_string())
                    && matches!(e.kind, crate::domain::entities::EntryKind::PickupDebit)
            });
            if still_absent {
                fresh.debit_pickup(debit_cents, payload.shipment_id, payload.pop_id);
                ledger_repo.save(&fresh).await
                    .context("Failed to save driver ledger after optimistic lock retry")?;
            }
        }
        Err(e) => return Err(e.context("Failed to save driver ledger")),
    }

    Ok(())
}
