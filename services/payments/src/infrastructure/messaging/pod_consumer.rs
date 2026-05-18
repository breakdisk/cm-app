//! Kafka consumer for `logisticos.pod.captured`.
//!
//! Routes each event through the unified `process_milestone_billing()` dispatcher,
//! which selects the correct strategy based on `billing_domain` in the payload.
//!
//! ## Domain Routing
//!
//! | billing_domain     | Milestone routed              | Strategy            |
//! |--------------------|-------------------------------|---------------------|
//! | `balikbayan`       | EmptyBoxDelivered / FullBoxRetrieved | BalikbayanStrategy |
//! | `standard_parcel`  | (handled by HubInboundConsumer) | StandardParcelStrategy |
//! | `cod`              | PodCaptured (cod_amount > 0)  | CodStrategy         |
//! | `merchant_periodic`| (handled by BillingAggregationService) | — |
//! | *(absent)*         | PodCaptured + booked_by_customer | Legacy PaymentReceipt path |
//!
//! ## Legacy Compatibility
//!
//! Events without a `billing_domain` field are handled by the pre-strategy path
//! (direct call to `InvoiceService::issue_payment_receipt`) so existing shipments
//! continue to work without re-migration.

use std::sync::Arc;
use anyhow::Context;
use logisticos_events::{
    consumer::KafkaConsumer,
    payloads::PodCaptured,
    topics::POD_CAPTURED,
};
use logisticos_types::TenantId;
use uuid::Uuid;

use crate::{
    application::{
        commands::IssuePaymentReceiptCommand,
        services::{CodService, InvoiceService},
    },
    domain::strategies::{
        process_milestone_billing,
        BillingDomain, MilestoneContext, PaymentMethod, ShipmentMilestone,
        BalikbayanStrategy, CodStrategy, StandardParcelStrategy,
    },
};

pub struct PodConsumer {
    inner:              KafkaConsumer,
    // ── Strategy instances ────────────────────────────────────────────────────
    balikbayan_strat:   Arc<BalikbayanStrategy>,
    standard_strat:     Arc<StandardParcelStrategy>,
    cod_strat:          Arc<CodStrategy>,
    // ── Legacy services (pre-strategy era) ───────────────────────────────────
    cod_service:        Arc<CodService>,
    invoice_service:    Arc<InvoiceService>,
}

impl PodConsumer {
    pub fn new(
        brokers:           &str,
        group_id:          &str,
        balikbayan_strat:  Arc<BalikbayanStrategy>,
        standard_strat:    Arc<StandardParcelStrategy>,
        cod_strat:         Arc<CodStrategy>,
        cod_service:       Arc<CodService>,
        invoice_service:   Arc<InvoiceService>,
    ) -> anyhow::Result<Self> {
        let inner = KafkaConsumer::new(brokers, group_id, &[POD_CAPTURED])
            .context("Failed to create PodConsumer Kafka client")?;
        Ok(Self { inner, balikbayan_strat, standard_strat, cod_strat, cod_service, invoice_service })
    }

    pub async fn run(self) {
        let balikbayan  = Arc::clone(&self.balikbayan_strat);
        let standard    = Arc::clone(&self.standard_strat);
        let cod         = Arc::clone(&self.cod_strat);
        let cod_svc     = Arc::clone(&self.cod_service);
        let inv_svc     = Arc::clone(&self.invoice_service);

        let result = self.inner.run(move |_topic, json| {
            let b = Arc::clone(&balikbayan);
            let s = Arc::clone(&standard);
            let c = Arc::clone(&cod);
            let cs = Arc::clone(&cod_svc);
            let is = Arc::clone(&inv_svc);
            async move { handle_pod_captured(b, s, c, cs, is, json).await }
        }).await;

        if let Err(e) = result {
            tracing::error!("PodConsumer loop exited with error: {e}");
        }
    }
}

async fn handle_pod_captured(
    balikbayan_strat:  Arc<BalikbayanStrategy>,
    standard_strat:    Arc<StandardParcelStrategy>,
    cod_strat:         Arc<CodStrategy>,
    cod_service:       Arc<CodService>,
    invoice_service:   Arc<InvoiceService>,
    json:              serde_json::Value,
) -> anyhow::Result<()> {
    let payload: PodCaptured = serde_json::from_value(
        json.get("data").cloned().unwrap_or(json.clone()),
    ).context("Failed to deserialise PodCaptured payload")?;

    let tenant_id_raw = json.get("tenant_id").and_then(|v| v.as_str())
        .or_else(|| json.get("data").and_then(|d| d.get("tenant_id")).and_then(|v| v.as_str()));

    let tenant_id = parse_tenant_id(tenant_id_raw, payload.pod_id)?;

    // ── Detect billing domain from event payload ───────────────────────────────
    let billing_domain_str = payload.billing_domain.as_deref().unwrap_or("");
    let billing_domain = BillingDomain::from_str(billing_domain_str);

    match billing_domain {
        // ── Track A: Balikbayan ──────────────────────────────────────────────
        Some(BillingDomain::Balikbayan) => {
            let milestone = match payload.balikbayan_stage.as_deref() {
                Some("stage_2_full_box") => ShipmentMilestone::BalikbayanFullBoxRetrieved,
                _                         => ShipmentMilestone::BalikbayanEmptyBoxDelivered,
            };

            let ctx = build_context(&tenant_id, &payload, milestone, BillingDomain::Balikbayan);

            let outcome = process_milestone_billing(
                &ctx, &balikbayan_strat, &standard_strat, &cod_strat,
            ).await.map_err(|e| anyhow::anyhow!("Balikbayan billing failed: {e:?}"))?;

            tracing::info!(
                shipment_id = %ctx.shipment_id,
                outcome     = ?std::mem::discriminant(&outcome),
                "Balikbayan milestone billing complete"
            );
        }

        // ── Track B: Standard Parcel — POD is not the billing trigger ─────────
        // Hub Inbound is the real trigger. POD arrival here is a no-op for billing.
        Some(BillingDomain::StandardParcel) => {
            tracing::debug!(
                shipment_id = %payload.shipment_id,
                "StandardParcel: pod.captured not a billing trigger — awaiting hub_inbound"
            );
        }

        // ── Track C: COD or absent billing_domain ─────────────────────────────
        Some(BillingDomain::Cod) | None => {
            // New strategy path for COD
            if payload.cod_amount_cents > 0 {
                let ctx = build_context(
                    &tenant_id, &payload,
                    ShipmentMilestone::PodCaptured,
                    BillingDomain::Cod,
                );
                let outcome = process_milestone_billing(
                    &ctx, &balikbayan_strat, &standard_strat, &cod_strat,
                ).await.map_err(|e| anyhow::anyhow!("COD billing failed: {e:?}"))?;

                tracing::info!(
                    shipment_id = %ctx.shipment_id,
                    cod_cents   = payload.cod_amount_cents,
                    outcome     = ?std::mem::discriminant(&outcome),
                    "COD milestone billing complete"
                );
            }

            // Legacy: customer-booked payment receipt (booked_by_customer == true, no COD)
            if payload.booked_by_customer && payload.cod_amount_cents == 0 {
                let customer_id = payload.customer_id
                    .context("pod.captured missing customer_id for customer-booked shipment")?;

                let delivered_on = chrono::DateTime::parse_from_rfc3339(&payload.captured_at)
                    .map(|dt| dt.date_naive())
                    .unwrap_or_else(|_| chrono::Utc::now().date_naive());

                let tenant_code = if payload.tenant_code.is_empty() {
                    "PH1".to_owned()
                } else {
                    payload.tenant_code.clone()
                };

                tracing::info!(
                    pod_id      = %payload.pod_id,
                    shipment_id = %payload.shipment_id,
                    customer_id = %customer_id,
                    "Issuing legacy payment receipt for customer-booked delivery"
                );

                invoice_service
                    .issue_payment_receipt(
                        &tenant_id,
                        IssuePaymentReceiptCommand {
                            shipment_id:    payload.shipment_id,
                            tenant_code,
                            customer_id,
                            customer_email: payload.customer_email.clone(),
                            delivered_on,
                            customer_name:  payload.customer_name.clone(),
                            customer_phone: payload.customer_phone.clone(),
                        },
                    )
                    .await
                    .map_err(|e| anyhow::anyhow!("Payment receipt issuance failed: {e:?}"))?;
            }
        }

        Some(BillingDomain::MerchantPeriodic) => {
            tracing::debug!(
                shipment_id = %payload.shipment_id,
                "MerchantPeriodic: pod.captured not a direct billing trigger"
            );
        }
    }

    Ok(())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn build_context(
    tenant_id: &TenantId,
    payload:   &PodCaptured,
    milestone: ShipmentMilestone,
    domain:    BillingDomain,
) -> MilestoneContext {
    MilestoneContext {
        tenant_id:    tenant_id.clone(),
        tenant_code:  if payload.tenant_code.is_empty() { "PH1".into() }
                      else { payload.tenant_code.clone() },
        shipment_id:  payload.shipment_id,
        pod_id:       payload.pod_id,
        driver_id:    payload.driver_id,
        shift_id:     payload.shift_id,
        milestone,
        domain,

        collected_amount_cents: payload.cod_amount_cents,
        currency:               logisticos_types::Currency::PHP, // override from billing source
        payment_method:         PaymentMethod::CashPhysical,

        customer_id:    payload.customer_id,
        customer_name:  payload.customer_name.clone(),
        customer_phone: payload.customer_phone.clone(),
        customer_email: payload.customer_email.clone(),

        // Balikbayan-specific
        box_id:              payload.box_id.clone(),
        box_size:            payload.box_size.clone(),
        ocean_freight_cents: payload.ocean_freight_cents,
        deposit_paid_cents:  payload.deposit_paid_cents,
        stage_1_invoice_id:  payload.stage_1_invoice_id,

        // Standard parcel
        actual_weight_grams:   payload.actual_weight_grams,
        declared_weight_grams: payload.declared_weight_grams,

        // Billing breakdown (fetched inside strategy from billing_source)
        base_freight_cents:   0,
        fuel_surcharge_cents: 0,
        insurance_cents:      0,
        tracking_number:      payload.tracking_number.clone(),
    }
}

fn parse_tenant_id(raw: Option<&str>, pod_id: Uuid) -> anyhow::Result<TenantId> {
    match raw {
        Some(id) => id.parse::<Uuid>()
            .context("Invalid tenant_id UUID in pod.captured")
            .map(TenantId::from_uuid),
        None => {
            tracing::warn!(pod_id = %pod_id, "pod.captured missing tenant_id");
            anyhow::bail!("pod.captured missing tenant_id")
        }
    }
}
