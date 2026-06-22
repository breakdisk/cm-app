//! Kafka consumer for `logisticos.driver.delivery.completed`.
//!
//! When a gig/part-time driver completes a delivery for a carrier-linked driver:
//!  1. Credits the carrier wallet with the driver's snapshotted payout_cents
//!     (the delivery margin the partner earns per drop).
//!  2. If COD was collected and a COD commission rate is configured, credits the
//!     carrier wallet with the calculated COD commission.
//!
//! Full-time drivers (payout_cents IS NULL) and unlinked drivers (carrier_id IS NULL)
//! produce no carrier wallet credit — they are tenant-owned and salaried.
//!
//! Idempotency: the wallet_transactions table has no unique constraint on reference_id,
//! but the wallet's optimistic-lock version ensures no double-credit on concurrent replays.

use std::sync::Arc;
use anyhow::Context;
use logisticos_events::{consumer::KafkaConsumer, topics::DELIVERY_COMPLETED};
use logisticos_types::{Currency, Money, TenantId};
use uuid::Uuid;

use crate::{
    domain::{
        entities::{Wallet, WalletTransaction},
        repositories::WalletRepository,
    },
};

pub struct DeliveryCompletedConsumer {
    inner:       KafkaConsumer,
    wallet_repo: Arc<dyn WalletRepository>,
}

impl DeliveryCompletedConsumer {
    pub fn new(
        brokers:     &str,
        group_id:    &str,
        wallet_repo: Arc<dyn WalletRepository>,
    ) -> anyhow::Result<Self> {
        let inner = KafkaConsumer::new(brokers, group_id, &[DELIVERY_COMPLETED])
            .context("Failed to create DeliveryCompletedConsumer Kafka client")?;
        Ok(Self { inner, wallet_repo })
    }

    pub async fn run(self) {
        let wallet_repo = self.wallet_repo;
        let result = self.inner.run(move |_topic, json| {
            let repo = Arc::clone(&wallet_repo);
            async move { handle_delivery_completed(repo, json).await }
        }).await;

        if let Err(e) = result {
            tracing::error!("DeliveryCompletedConsumer loop exited with error: {e}");
        }
    }
}

async fn handle_delivery_completed(
    wallet_repo: Arc<dyn WalletRepository>,
    json:        serde_json::Value,
) -> anyhow::Result<()> {
    // Deserialise from envelope: payload lives under "data" key.
    let data = json.get("data").cloned().unwrap_or(json.clone());

    let task_id:    Uuid = serde_json::from_value(data["task_id"].clone())
        .context("delivery.completed missing task_id")?;
    let tenant_id_raw: Uuid = serde_json::from_value(data["tenant_id"].clone())
        .context("delivery.completed missing tenant_id")?;
    let carrier_id: Option<Uuid> = data["carrier_id"].as_str()
        .and_then(|s| s.parse().ok());
    let payout_cents: Option<i64> = data["payout_cents"].as_i64();
    let cod_amount_cents: i64 = data["cod_amount_cents"].as_i64().unwrap_or(0);
    let cod_commission_rate_bps: i64 = data["cod_commission_rate_bps"].as_i64().unwrap_or(0);

    // Only carrier-linked gig drivers generate carrier wallet credits.
    let carrier_id = match carrier_id {
        Some(id) => id,
        None => {
            tracing::debug!(task_id = %task_id, "delivery.completed — no carrier_id, skipping carrier settlement");
            return Ok(());
        }
    };

    let tenant_id = TenantId::from_uuid(tenant_id_raw);

    // ── Delivery margin credit ────────────────────────────────────────────────
    if let Some(payout) = payout_cents.filter(|&p| p > 0) {
        credit_carrier_wallet(
            &wallet_repo,
            &tenant_id,
            carrier_id,
            task_id,
            payout,
            WalletTransaction::carrier_margin,
            "delivery margin",
        ).await?;
    }

    // ── COD commission credit ─────────────────────────────────────────────────
    if cod_amount_cents > 0 && cod_commission_rate_bps > 0 {
        let commission = cod_amount_cents * cod_commission_rate_bps / 10_000;
        if commission > 0 {
            credit_carrier_wallet(
                &wallet_repo,
                &tenant_id,
                carrier_id,
                task_id,
                commission,
                WalletTransaction::cod_commission,
                "COD commission",
            ).await?;
        }
    }

    Ok(())
}

async fn credit_carrier_wallet(
    wallet_repo:  &Arc<dyn WalletRepository>,
    tenant_id:    &TenantId,
    carrier_id:   Uuid,
    task_id:      Uuid,
    amount_cents: i64,
    tx_factory:   fn(Uuid, TenantId, Money, Uuid) -> WalletTransaction,
    label:        &str,
) -> anyhow::Result<()> {
    // Find or provision the carrier wallet. Provisioning is optimistic — if two
    // events race to create, the second sees the wallet via find and uses it.
    let mut wallet = match wallet_repo.find_carrier_wallet(tenant_id, carrier_id).await
        .context("find carrier wallet")?
    {
        Some(w) => w,
        None => {
            let w = Wallet::new(tenant_id.clone(), Currency::PHP);
            // Best-effort insert — ignore conflict (concurrent creation).
            let _ = wallet_repo.save_carrier_wallet(&w, carrier_id).await;
            // Re-fetch so we have the real wallet_id after any conflict resolution.
            wallet_repo
                .find_carrier_wallet(tenant_id, carrier_id).await
                .context("re-fetch carrier wallet after provision")?
                .context("carrier wallet missing after provision")?
        }
    };

    let amount = Money::new(amount_cents, Currency::PHP);
    wallet.credit(amount).map_err(|e| anyhow::anyhow!("carrier wallet credit failed: {e}"))?;

    // Retry once on optimistic-lock conflict (concurrent delivery completions).
    match wallet_repo.save_carrier_wallet(&wallet, carrier_id).await {
        Ok(()) => {}
        Err(e) if e.to_string().contains("optimistic lock") => {
            tracing::warn!(
                carrier_id = %carrier_id,
                task_id    = %task_id,
                "Carrier wallet optimistic lock on {label} — reloading and retrying"
            );
            let mut fresh = wallet_repo
                .find_carrier_wallet(tenant_id, carrier_id).await
                .context("reload carrier wallet")?
                .context("carrier wallet missing on retry")?;
            fresh.credit(amount).map_err(|e| anyhow::anyhow!("retry credit failed: {e}"))?;
            wallet_repo.save_carrier_wallet(&fresh, carrier_id).await
                .context("save carrier wallet on retry")?;
            wallet = fresh;
        }
        Err(e) => return Err(e.context(format!("save carrier wallet for {label}"))),
    }

    let tx = tx_factory(wallet.id, tenant_id.clone(), amount, task_id);
    wallet_repo.record_transaction(&tx).await
        .context(format!("record {label} wallet transaction"))?;

    tracing::info!(
        carrier_id   = %carrier_id,
        task_id      = %task_id,
        amount_cents = amount_cents,
        label        = label,
        "Carrier wallet credited"
    );
    Ok(())
}
