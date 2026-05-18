//! Track B — Standard Parcel / Courier Billing Strategy (B2B/B2C Prepaid Marketplace)
//!
//! ## Invoice Lifecycle
//!
//! ```text
//! SHIPMENT_CREATED       → $0 event. Pickup is free. Invoice not yet generated.
//!                          workflow_metadata records declared weight/dims for later comparison.
//!
//! HUB_INBOUND_SCANNED    → Hub scale measures actual weight.
//!   IF actual == declared → Issue standard freight invoice (prepaid/credit terms).
//!                           Shipment proceeds normally.
//!   IF actual != declared → Issue BOTH:
//!                              (a) original freight invoice
//!                              (b) weight_overage_surcharge invoice
//!                           Flag shipment HELD_FOR_PAYMENT_OVERAGE.
//!                           Emit: standard_parcel.held_for_payment_overage
//!                           Driver CANNOT release package until merchant pays.
//!
//! PAYMENT_CONFIRMED       → Overage invoice marked Paid. Hold lifted.
//!                           Emit: standard_parcel.hold_released
//! ```
//!
//! ## Enforcement Rule
//!
//! Carrier / Driver-Ops service must check `payments.billing_clearance` before
//! allowing a package to be released. If `is_cleared = false`, return HTTP 402
//! with `unpaid_invoice_ids` in the response body.

use std::sync::Arc;
use async_trait::async_trait;
use chrono::Utc;
use logisticos_errors::{AppError, AppResult};
use logisticos_events::{envelope::Event, producer::KafkaProducer, topics};
use logisticos_types::{
    invoice::{ChargeType, InvoiceNumber, InvoiceType},
    Currency, MerchantId, Money,
};
use uuid::Uuid;

use crate::{
    application::services::InvoiceSequenceSource,
    domain::{
        entities::{BillingPeriod, Invoice, InvoiceLineItem},
        events::{StandardParcelInvoiced, StandardParcelHeldForOverage},
        repositories::{InvoiceRepository, ShipmentBillingSource},
        strategies::{BillingStrategy, MilestoneContext, MilestoneOutcome, ShipmentMilestone},
    },
};

// ── Weight surcharge rate ─────────────────────────────────────────────────────

/// Overage surcharge rate per gram over the declared weight.
/// Loaded from env `WEIGHT_SURCHARGE_CENTS_PER_KG` (default: 5000 = PHP 50/kg).
fn weight_surcharge_cents_per_gram() -> i64 {
    std::env::var("WEIGHT_SURCHARGE_CENTS_PER_KG")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(5000)  // PHP 50 per kg default
        / 1000            // convert to per-gram
}

// ── StandardParcelStrategy ────────────────────────────────────────────────────

pub struct StandardParcelStrategy {
    invoice_repo:    Arc<dyn InvoiceRepository>,
    kafka:           Arc<KafkaProducer>,
    sequence_source: Arc<dyn InvoiceSequenceSource>,
    billing_source:  Arc<dyn ShipmentBillingSource>,
}

impl StandardParcelStrategy {
    pub fn new(
        invoice_repo:    Arc<dyn InvoiceRepository>,
        kafka:           Arc<KafkaProducer>,
        sequence_source: Arc<dyn InvoiceSequenceSource>,
        billing_source:  Arc<dyn ShipmentBillingSource>,
    ) -> Self {
        Self { invoice_repo, kafka, sequence_source, billing_source }
    }

    // ── Hub Inbound: Issue invoice (with or without overage) ──────────────────

    async fn process_hub_inbound(
        &self,
        ctx: &MilestoneContext,
    ) -> AppResult<MilestoneOutcome> {
        let today = Utc::now().date_naive();

        // ── Fetch declared freight breakdown from order-intake ────────────────
        let billing = self.billing_source
            .fetch(ctx.shipment_id)
            .await
            .map_err(AppError::Internal)?;

        let currency = parse_currency(&billing.currency);

        // Idempotency: if a freight invoice already exists for this shipment, skip.
        if self.invoice_repo
            .exists_for_shipment_and_domain(ctx.shipment_id, "standard_parcel")
            .await
            .map_err(AppError::Internal)?
        {
            tracing::warn!(
                shipment_id = %ctx.shipment_id,
                "StandardParcel: hub-inbound invoice already exists — skipping (idempotent)"
            );
            return Ok(MilestoneOutcome::NoAction);
        }

        // ── Generate the base freight invoice ────────────────────────────────
        let seq = self.sequence_source
            .next_sequence(InvoiceType::ShipmentCharges, &ctx.tenant_code, today)
            .await.map_err(AppError::Internal)?;

        let inv_number = InvoiceNumber::generate(
            InvoiceType::ShipmentCharges, &ctx.tenant_code, today, seq,
        ).map_err(|e| AppError::Internal(anyhow::anyhow!(e.to_string())))?;

        // merchant_id comes from billing source.
        let merchant_id = MerchantId::from_uuid(billing.merchant_id);

        let mut invoice = Invoice::new(
            inv_number,
            InvoiceType::ShipmentCharges,
            ctx.tenant_id.clone(),
            merchant_id,
            None,
            BillingPeriod::single_day(today),
            currency,
        );

        // Base freight line item
        if billing.base_freight_cents > 0 {
            invoice.add_line_item(InvoiceLineItem::document_level(
                ChargeType::HubInboundFreight,
                format!("Hub inbound freight — AWB {}", ctx.tracking_number),
                Money::new(billing.base_freight_cents, currency),
            )).map_err(|e| AppError::Internal(anyhow::anyhow!(e.to_string())))?;
        }
        if billing.fuel_surcharge_cents > 0 {
            invoice.add_line_item(InvoiceLineItem::document_level(
                ChargeType::FuelSurcharge,
                "Fuel surcharge".into(),
                Money::new(billing.fuel_surcharge_cents, currency),
            )).map_err(|e| AppError::Internal(anyhow::anyhow!(e.to_string())))?;
        }
        if billing.insurance_cents > 0 {
            invoice.add_line_item(InvoiceLineItem::document_level(
                ChargeType::InsuranceFee,
                "Shipment insurance".into(),
                Money::new(billing.insurance_cents, currency),
            )).map_err(|e| AppError::Internal(anyhow::anyhow!(e.to_string())))?;
        }

        if invoice.line_items.is_empty() {
            return Err(AppError::BusinessRule(
                format!("StandardParcel: shipment {} billing returned all-zero amounts", ctx.tracking_number),
            ));
        }

        invoice.issue().map_err(|e| AppError::BusinessRule(e.to_string()))?;

        // ── Detect weight overage ─────────────────────────────────────────────
        let actual_g   = ctx.actual_weight_grams.unwrap_or(0);
        let declared_g = ctx.declared_weight_grams.unwrap_or(0);
        let has_overage = actual_g > declared_g && declared_g > 0;

        let overage_cents = if has_overage {
            let delta_grams = (actual_g - declared_g) as i64;
            delta_grams * weight_surcharge_cents_per_gram()
        } else {
            0
        };

        // Metadata for billing_clearance and HELD enforcement
        let metadata = if has_overage {
            serde_json::json!({
                "domain":                 "standard_parcel",
                "shipment_id":             ctx.shipment_id,
                "declared_weight_grams":   declared_g,
                "actual_weight_grams":     actual_g,
                "overage_surcharge_cents": overage_cents,
                "held_since":              Utc::now().to_rfc3339(),
            })
        } else {
            serde_json::json!({
                "domain":                 "standard_parcel",
                "shipment_id":             ctx.shipment_id,
                "declared_weight_grams":   declared_g,
                "actual_weight_grams":     actual_g,
            })
        };

        let invoice_id     = invoice.id.inner();
        let inv_number_str = invoice.invoice_number.to_string();
        let base_total     = invoice.total_due().amount;

        self.invoice_repo
            .save_with_domain(&invoice, "standard_parcel", metadata)
            .await
            .map_err(AppError::Internal)?;

        // ── If overage: generate a second invoice and hold shipment ───────────
        if has_overage {
            let overage_seq = self.sequence_source
                .next_sequence(InvoiceType::ShipmentCharges, &ctx.tenant_code, today)
                .await.map_err(AppError::Internal)?;

            let overage_number = InvoiceNumber::generate(
                InvoiceType::ShipmentCharges, &ctx.tenant_code, today, overage_seq,
            ).map_err(|e| AppError::Internal(anyhow::anyhow!(e.to_string())))?;

            let mut overage_inv = Invoice::new(
                overage_number,
                InvoiceType::ShipmentCharges,
                ctx.tenant_id.clone(),
                invoice.merchant_id.clone(),
                None,
                BillingPeriod::single_day(today),
                currency,
            );

            overage_inv.add_line_item(InvoiceLineItem::document_level(
                ChargeType::WeightOverageSurcharge,
                format!(
                    "Weight overage surcharge — declared {}g, actual {}g (+{}g) AWB {}",
                    declared_g, actual_g, actual_g - declared_g, ctx.tracking_number
                ),
                Money::new(overage_cents, currency),
            )).map_err(|e| AppError::Internal(anyhow::anyhow!(e.to_string())))?;

            overage_inv.issue().map_err(|e| AppError::BusinessRule(e.to_string()))?;

            let overage_invoice_id = overage_inv.id.inner();

            self.invoice_repo
                .save_with_domain(
                    &overage_inv,
                    "standard_parcel",
                    serde_json::json!({
                        "domain":                  "standard_parcel",
                        "shipment_id":              ctx.shipment_id,
                        "overage_type":             "weight_mismatch",
                        "declared_weight_grams":    declared_g,
                        "actual_weight_grams":      actual_g,
                        "overage_surcharge_cents":  overage_cents,
                        "parent_invoice_id":        invoice_id,
                    }),
                )
                .await
                .map_err(AppError::Internal)?;

            // ── Kafka: standard_parcel.held_for_payment_overage ───────────────
            let event = Event::new(
                "payments",
                topics::STANDARD_PARCEL_HELD_FOR_OVERAGE,
                ctx.tenant_id.inner(),
                StandardParcelHeldForOverage {
                    freight_invoice_id:    invoice_id,
                    overage_invoice_id,
                    shipment_id:           ctx.shipment_id,
                    tracking_number:       ctx.tracking_number.clone(),
                    declared_weight_grams: declared_g,
                    actual_weight_grams:   actual_g,
                    overage_surcharge_cents: overage_cents,
                    base_freight_cents:    base_total,
                    currency:              format!("{currency:?}"),
                    tenant_id:             ctx.tenant_id.inner(),
                    held_since:            Utc::now().to_rfc3339(),
                },
            );
            self.kafka
                .publish_event(topics::STANDARD_PARCEL_HELD_FOR_OVERAGE, &event)
                .await
                .map_err(AppError::Internal)?;

            tracing::warn!(
                invoice_id             = %invoice_id,
                overage_invoice_id     = %overage_invoice_id,
                declared_weight_grams  = declared_g,
                actual_weight_grams    = actual_g,
                overage_surcharge_cents = overage_cents,
                tracking_number        = %ctx.tracking_number,
                "StandardParcel: weight mismatch — shipment HELD_FOR_PAYMENT_OVERAGE"
            );

            return Ok(MilestoneOutcome::ShipmentHeld {
                invoice_id: overage_invoice_id,
                overage_cents,
                reason: format!(
                    "Weight mismatch: declared {}g, actual {}g. \
                     Overage surcharge {} cents must be paid before release.",
                    declared_g, actual_g, overage_cents
                ),
            });
        }

        // ── No overage: publish standard invoiced event ───────────────────────
        let event = Event::new(
            "payments",
            topics::STANDARD_PARCEL_INVOICED,
            ctx.tenant_id.inner(),
            StandardParcelInvoiced {
                invoice_id,
                invoice_number:  inv_number_str.clone(),
                shipment_id:     ctx.shipment_id,
                tracking_number: ctx.tracking_number.clone(),
                total_cents:     base_total,
                currency:        format!("{currency:?}"),
                tenant_id:       ctx.tenant_id.inner(),
                issued_at:       Utc::now().to_rfc3339(),
            },
        );
        self.kafka
            .publish_event(topics::STANDARD_PARCEL_INVOICED, &event)
            .await
            .map_err(AppError::Internal)?;

        tracing::info!(
            invoice_id      = %invoice_id,
            total_cents     = base_total,
            tracking_number = %ctx.tracking_number,
            "StandardParcel: hub-inbound freight invoice issued"
        );

        Ok(MilestoneOutcome::InvoiceIssued {
            invoice_id,
            invoice_number:  inv_number_str,
            total_cents:     base_total,
            ledger_updated:  false,  // prepaid/credit terms — no driver cash
        })
    }
}

#[async_trait]
impl BillingStrategy for StandardParcelStrategy {
    async fn process_milestone(
        &self,
        ctx: &MilestoneContext,
    ) -> AppResult<MilestoneOutcome> {
        match ctx.milestone {
            ShipmentMilestone::HubInboundScanned
            | ShipmentMilestone::WeightDiscrepancyDetected => {
                self.process_hub_inbound(ctx).await
            }
            // Pickup, transit, delivery — all $0 for StandardParcel billing.
            _ => {
                tracing::debug!(
                    shipment_id = %ctx.shipment_id,
                    milestone   = ?ctx.milestone,
                    "StandardParcel: milestone not billed"
                );
                Ok(MilestoneOutcome::NoAction)
            }
        }
    }

    fn domain_name(&self) -> &'static str { "standard_parcel" }
}

fn parse_currency(s: &str) -> Currency {
    match s.to_uppercase().as_str() {
        "AED" => Currency::AED,
        "USD" => Currency::USD,
        "SGD" => Currency::SGD,
        "MYR" => Currency::MYR,
        "IDR" => Currency::IDR,
        _     => Currency::PHP,
    }
}
