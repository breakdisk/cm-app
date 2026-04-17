//! CodService — records cash-on-delivery collections at POD time.
//!
//! **What this service does:** creates a `CodCollection` row in
//! `collected` status and emits `cod.collected`. That's it.
//!
//! **What this service does NOT do:** credit the merchant wallet.
//! Wallet credit happens in `CodRemittanceService::confirm_batch` once the
//! physical cash has been handed to finance. Collection and remittance are
//! intentionally decoupled — a driver carrying cash is not yet the platform's
//! cash, and platform cashflow must reflect that.

use std::sync::Arc;
use logisticos_errors::{AppError, AppResult};
use logisticos_types::{Currency, MerchantId, Money, TenantId};
use logisticos_events::{producer::KafkaProducer, topics, envelope::Event};

use crate::{
    application::commands::ReconcileCodCommand,
    domain::{
        entities::CodCollection,
        events::CodReconciled,
        repositories::{CodRepository, ShipmentBillingSource},
    },
};

pub struct CodService {
    cod_repo:          Arc<dyn CodRepository>,
    shipment_source:   Arc<dyn ShipmentBillingSource>,
    kafka:             Arc<KafkaProducer>,
}

impl CodService {
    pub fn new(
        cod_repo:        Arc<dyn CodRepository>,
        shipment_source: Arc<dyn ShipmentBillingSource>,
        kafka:           Arc<KafkaProducer>,
    ) -> Self {
        Self { cod_repo, shipment_source, kafka }
    }

    /// Called by the POD consumer (or the internal wallet/reconcile route)
    /// when a driver reports COD collection on a delivered shipment.
    ///
    /// Records the collection; does NOT credit the merchant wallet.
    pub async fn reconcile_cod(
        &self,
        tenant_id: &TenantId,
        cmd: ReconcileCodCommand,
    ) -> AppResult<()> {
        if cmd.amount_cents <= 0 {
            return Err(AppError::BusinessRule("COD amount must be positive".into()));
        }

        // Idempotency: one COD record per shipment.
        if self.cod_repo.find_by_shipment(cmd.shipment_id).await
            .map_err(AppError::Internal)?
            .is_some()
        {
            tracing::warn!(
                shipment_id = %cmd.shipment_id,
                "COD already recorded for shipment — skipping",
            );
            return Ok(());
        }

        // Resolve merchant owner of the shipment from order-intake.
        // ShipmentBillingSource is our existing port; reusing it avoids a
        // second round-trip or a new per-merchant lookup API.
        let shipment = self.shipment_source
            .fetch(cmd.shipment_id)
            .await
            .map_err(AppError::Internal)?;
        let merchant_id = MerchantId::from_uuid(shipment.merchant_id);

        let amount = Money::new(cmd.amount_cents, Currency::PHP);
        let cod = CodCollection::new(
            tenant_id.clone(),
            merchant_id,
            cmd.shipment_id,
            cmd.driver_id,
            cmd.pod_id,
            amount,
        );
        let platform_fee    = cod.platform_fee().amount;
        let merchant_credit = cod.merchant_credit().amount;
        let cod_id          = cod.id;

        self.cod_repo.save(&cod).await.map_err(AppError::Internal)?;

        // cod.collected — the driver has the cash, batch/remit happens later.
        let event = Event::new(
            "payments",
            "cod.collected",
            tenant_id.inner(),
            CodReconciled {
                cod_id,
                shipment_id:           cmd.shipment_id,
                tenant_id:             tenant_id.inner(),
                amount_cents:          cmd.amount_cents,
                merchant_credit_cents: merchant_credit,
                platform_fee_cents:    platform_fee,
            },
        );
        self.kafka.publish_event(topics::COD_COLLECTED, &event)
            .await.map_err(AppError::Internal)?;

        tracing::info!(
            cod_id      = %cod_id,
            shipment_id = %cmd.shipment_id,
            merchant_id = %cod.merchant_id,
            amount      = cmd.amount_cents,
            "COD collection recorded (pending remittance)",
        );
        Ok(())
    }
}
