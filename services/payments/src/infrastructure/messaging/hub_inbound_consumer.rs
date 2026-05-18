//! Kafka consumer for `logisticos.hub.piece.scanned`.
//!
//! Triggers Track B (StandardParcel) freight invoice generation when a parcel
//! arrives at hub inbound.  Only `scan_type == "inbound"` events with
//! `billing_domain == "standard_parcel"` are acted on; all others are skipped.
//!
//! ## Idempotency
//!
//! The `StandardParcelStrategy` checks `InvoiceRepository::exists_paid_for_shipment_and_domain`
//! before writing anything.  Multi-piece shipments emit one `PieceScanned` event per piece —
//! the first piece triggers invoice generation; subsequent pieces are no-ops.
//!
//! ## Weight Discrepancy Path
//!
//! If the hub scale detects a mismatch, hub-ops emits a separate
//! `logisticos.hub.piece.weight_discrepancy` event handled by `WeightDiscrepancyConsumer`.
//! That consumer generates the overage surcharge invoice and flags `HELD_FOR_PAYMENT_OVERAGE`.
//! The hub-inbound consumer handles only the clean-scan base-freight path.

use std::sync::Arc;
use anyhow::Context;
use logisticos_events::{
    consumer::KafkaConsumer,
    payloads::PieceScanned,
    topics::PIECE_SCANNED,
};
use logisticos_types::{Currency, TenantId};
use uuid::Uuid;

use crate::domain::strategies::{
    process_milestone_billing,
    BillingDomain, MilestoneContext, PaymentMethod, ShipmentMilestone,
    BalikbayanStrategy, CodStrategy, StandardParcelStrategy,
};

pub struct HubInboundConsumer {
    inner:          KafkaConsumer,
    balikbayan_strat: Arc<BalikbayanStrategy>,
    standard_strat:   Arc<StandardParcelStrategy>,
    cod_strat:        Arc<CodStrategy>,
}

impl HubInboundConsumer {
    pub fn new(
        brokers:          &str,
        group_id:         &str,
        balikbayan_strat: Arc<BalikbayanStrategy>,
        standard_strat:   Arc<StandardParcelStrategy>,
        cod_strat:        Arc<CodStrategy>,
    ) -> anyhow::Result<Self> {
        let inner = KafkaConsumer::new(brokers, group_id, &[PIECE_SCANNED])
            .context("Failed to create HubInboundConsumer Kafka client")?;
        Ok(Self { inner, balikbayan_strat, standard_strat, cod_strat })
    }

    pub async fn run(self) {
        let b = Arc::clone(&self.balikbayan_strat);
        let s = Arc::clone(&self.standard_strat);
        let c = Arc::clone(&self.cod_strat);

        let result = self.inner.run(move |_topic, json| {
            let b = Arc::clone(&b);
            let s = Arc::clone(&s);
            let c = Arc::clone(&c);
            async move { handle_piece_scanned(b, s, c, json).await }
        }).await;

        if let Err(e) = result {
            tracing::error!("HubInboundConsumer loop exited with error: {e}");
        }
    }
}

async fn handle_piece_scanned(
    balikbayan_strat: Arc<BalikbayanStrategy>,
    standard_strat:   Arc<StandardParcelStrategy>,
    cod_strat:        Arc<CodStrategy>,
    json:             serde_json::Value,
) -> anyhow::Result<()> {
    let payload: PieceScanned = serde_json::from_value(
        json.get("data").cloned().unwrap_or(json.clone()),
    ).context("Failed to deserialise PieceScanned payload")?;

    // Only billing-relevant inbound scans
    if payload.scan_type != "inbound" {
        tracing::trace!(
            shipment_id = %payload.shipment_id,
            scan_type   = %payload.scan_type,
            "Skipping non-inbound piece scan"
        );
        return Ok(());
    }

    let billing_domain = match payload.billing_domain.as_deref() {
        Some("standard_parcel") => BillingDomain::StandardParcel,
        Some(other) => {
            tracing::debug!(
                shipment_id    = %payload.shipment_id,
                billing_domain = other,
                "HubInboundConsumer: non-standard-parcel domain — skipping"
            );
            return Ok(());
        }
        None => {
            tracing::debug!(
                shipment_id = %payload.shipment_id,
                "HubInboundConsumer: no billing_domain in PieceScanned — skipping"
            );
            return Ok(());
        }
    };

    // Only process the first piece to avoid duplicate invoice generation.
    // Subsequent pieces will hit the idempotency guard inside the strategy,
    // but we short-circuit here to save a DB round-trip per extra piece.
    if payload.piece_number > 1 {
        tracing::debug!(
            shipment_id   = %payload.shipment_id,
            piece_number  = payload.piece_number,
            "Skipping non-first piece (idempotency short-circuit)"
        );
        return Ok(());
    }

    let tenant_id = TenantId::from_uuid(payload.tenant_id);

    let ctx = MilestoneContext {
        tenant_id:   tenant_id.clone(),
        tenant_code: "PH1".into(),  // will be overridden by strategy from billing_source
        shipment_id: payload.shipment_id,
        // Hub scans don't carry pod_id/driver_id — use nil UUIDs as sentinels.
        // StandardParcelStrategy does not use these fields.
        pod_id:      Uuid::nil(),
        driver_id:   Uuid::nil(),
        shift_id:    None,
        milestone:   ShipmentMilestone::HubInboundScanned,
        domain:      billing_domain,

        collected_amount_cents: 0,  // prepaid — no cash collected at hub
        currency:    Currency::PHP,
        payment_method: PaymentMethod::PrepaidBalance,

        customer_id:    None,
        customer_name:  String::new(),
        customer_phone: String::new(),
        customer_email: None,

        // Balikbayan fields — not applicable for StandardParcel
        box_id:              None,
        box_size:            None,
        ocean_freight_cents: None,
        deposit_paid_cents:  None,
        stage_1_invoice_id:  None,

        // Weight fields — populated from billing_source inside StandardParcelStrategy
        actual_weight_grams:   None,
        declared_weight_grams: None,

        // Billing breakdown — fetched from order-intake inside the strategy
        base_freight_cents:   0,
        fuel_surcharge_cents: 0,
        insurance_cents:      0,
        tracking_number:      payload.master_awb.clone(),
    };

    let outcome = process_milestone_billing(
        &ctx, &balikbayan_strat, &standard_strat, &cod_strat,
    ).await.map_err(|e| anyhow::anyhow!("StandardParcel hub-inbound billing failed: {e:?}"))?;

    tracing::info!(
        shipment_id  = %payload.shipment_id,
        master_awb   = %payload.master_awb,
        outcome      = ?std::mem::discriminant(&outcome),
        "Hub-inbound billing milestone processed"
    );

    Ok(())
}
