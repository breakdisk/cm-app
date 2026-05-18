//! Unified Billing Strategy — single dispatcher for all three logistics payment tracks.
//!
//! # Design
//!
//! The `BillingStrategy` trait is the **Strategy** in the Strategy Pattern.
//! Each domain variant (`Balikbayan`, `StandardParcel`, `Cod`) is a concrete strategy
//! that encodes its own milestone-to-invoice mapping, enforcement gates, and ledger
//! debits — all behind one function call: `process_milestone_billing()`.
//!
//! ```text
//! process_milestone_billing(ctx)
//!   └─ match ctx.domain {
//!        BillingDomain::Balikbayan      → BalikbayanStrategy::process_milestone()
//!        BillingDomain::StandardParcel  → StandardParcelStrategy::process_milestone()
//!        BillingDomain::Cod             → CodStrategy::process_milestone()
//!      }
//! ```
//!
//! # Invariants
//! - Strategies are **stateless** — all mutable state lives in `Invoice` + `DriverLedger`.
//! - Strategies are **idempotent** — duplicate milestone events produce no side-effects
//!   (guarded by `ON CONFLICT DO NOTHING` at the DB layer + `find_by_*` checks).
//! - No strategy makes synchronous cross-service HTTP calls — all enforcement is
//!   expressed as a `MilestoneOutcome::ManifestLocked` or `ManifestCleared` variant
//!   that the HTTP handler translates to the appropriate response code.

pub mod balikbayan;
pub mod cod;
pub mod standard_parcel;

use async_trait::async_trait;
use logisticos_errors::AppResult;
use logisticos_types::{Currency, TenantId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub use balikbayan::BalikbayanStrategy;
pub use cod::CodStrategy;
pub use standard_parcel::StandardParcelStrategy;

// ── BillingDomain ─────────────────────────────────────────────────────────────

/// Identifies which payment-track a shipment belongs to.
///
/// Carried on the `invoices.billing_domain` column and, at the Kafka layer,
/// in the `billing_domain` field of `PodCaptured` / `HubInboundScanned` events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BillingDomain {
    /// Track A — Sea-freight Balikbayan box (UAE → PH corridor).
    /// Two-stage cash collection: deposit on empty-box delivery, balance on full-box pickup.
    Balikbayan,
    /// Track B — B2B/B2C prepaid marketplace parcel.
    /// Invoice triggered at Hub Inbound; weight-overage auto-hold on mismatch.
    StandardParcel,
    /// Track C — Cash on Delivery / last-mile courier.
    /// Invoice triggered exclusively at Final Delivery; mobile-lock enforcement.
    Cod,
    /// Default — periodic merchant invoice (existing flow, not routed through strategy).
    MerchantPeriodic,
}

impl BillingDomain {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "balikbayan"       => Some(Self::Balikbayan),
            "standard_parcel"  => Some(Self::StandardParcel),
            "cod"              => Some(Self::Cod),
            "merchant_periodic"=> Some(Self::MerchantPeriodic),
            _                  => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Balikbayan      => "balikbayan",
            Self::StandardParcel  => "standard_parcel",
            Self::Cod             => "cod",
            Self::MerchantPeriodic=> "merchant_periodic",
        }
    }
}

// ── ShipmentMilestone ─────────────────────────────────────────────────────────

/// A billing-relevant event in the shipment lifecycle.
///
/// Not every milestone triggers billing for every domain — the strategy decides.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShipmentMilestone {
    // ── Track A (Balikbayan) ─────────────────────────────────────────────────
    /// Driver delivered an empty box and collected the cash deposit.
    BalikbayanEmptyBoxDelivered,
    /// Driver retrieved the full box, measured dimensions, collected balance.
    BalikbayanFullBoxRetrieved,

    // ── Track B (Standard Parcel) ────────────────────────────────────────────
    /// Package scanned at hub inbound — this is when the invoice is generated.
    HubInboundScanned,
    /// Hub scale found a weight/dimension mismatch vs. declared.
    WeightDiscrepancyDetected,

    // ── Track C (COD) ────────────────────────────────────────────────────────
    /// Driver completed doorstep delivery, collected cash/digital payment.
    CodDeliveryCompleted,

    // ── Shared ───────────────────────────────────────────────────────────────
    /// POD submitted — used as the unified trigger for all tracks.
    PodCaptured,
}

// ── MilestoneContext ──────────────────────────────────────────────────────────

/// All data available to a strategy when processing a billing milestone.
///
/// Populated from the Kafka event payload; strategies must not make external calls
/// to fill missing fields — use denormalized event payloads instead.
#[derive(Debug, Clone)]
pub struct MilestoneContext {
    pub tenant_id:    TenantId,
    pub tenant_code:  String,      // 3-char, e.g. "UAE1" / "PH1"
    pub shipment_id:  Uuid,
    pub pod_id:       Uuid,
    pub driver_id:    Uuid,
    pub shift_id:     Option<Uuid>,
    pub milestone:    ShipmentMilestone,
    pub domain:       BillingDomain,

    // ── Financial ────────────────────────────────────────────────────────────
    /// Amount the driver physically collected (cash or digital, in minor currency units).
    pub collected_amount_cents: i64,
    pub currency:     Currency,
    pub payment_method: PaymentMethod,

    // ── Customer / recipient ──────────────────────────────────────────────────
    pub customer_id:    Option<Uuid>,
    pub customer_name:  String,
    pub customer_phone: String,
    pub customer_email: Option<String>,

    // ── Domain-specific fields ────────────────────────────────────────────────
    /// Track A only: unique identifier for the physical box (e.g., sticker barcode).
    pub box_id:       Option<String>,
    /// Track A only: box size code (small/medium/large/jumbo) used to look up flat rate.
    pub box_size:     Option<String>,
    /// Track A only: flat ocean freight rate for the detected box size (cents).
    pub ocean_freight_cents: Option<i64>,
    /// Track A only: deposit amount already paid at Stage 1.
    pub deposit_paid_cents: Option<i64>,
    /// Track A only: Stage 1 invoice ID — needed to credit the deposit in Stage 2.
    pub stage_1_invoice_id: Option<Uuid>,

    /// Track B only: hub-measured actual weight in grams.
    pub actual_weight_grams: Option<u32>,
    /// Track B only: merchant-declared weight in grams.
    pub declared_weight_grams: Option<u32>,

    // ── Shipment billing breakdown (pre-fetched from order-intake) ────────────
    pub base_freight_cents:  i64,
    pub fuel_surcharge_cents: i64,
    pub insurance_cents:     i64,
    pub tracking_number:     String,
}

// ── PaymentMethod ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentMethod {
    CashPhysical,
    MobileCreditCard,
    DigitalWalletQr,
    PrepaidBalance,
    CreditTerms,
}

// ── MilestoneOutcome ──────────────────────────────────────────────────────────

/// Result of processing a billing milestone.
///
/// HTTP handlers translate this into appropriate response codes and Kafka events.
#[derive(Debug)]
pub enum MilestoneOutcome {
    /// Invoice issued and (for cash tracks) driver liability recorded.
    InvoiceIssued {
        invoice_id:    Uuid,
        invoice_number: String,
        total_cents:   i64,
        /// Driver ledger updated — balance is now more negative.
        ledger_updated: bool,
    },
    /// Stage 1 of a two-stage workflow completed (Balikbayan deposit).
    DepositRecorded {
        invoice_id:       Uuid,
        deposit_cents:    i64,
    },
    /// Invoice generated AND manifest can now be released (both Balikbayan stages paid).
    ManifestCleared {
        invoice_id:       Uuid,
        invoice_number:   String,
        total_cents:      i64,
    },
    /// Shipment is held — driver must not release package until payment clears.
    ShipmentHeld {
        invoice_id:       Uuid,
        overage_cents:    i64,
        reason:           String,
    },
    /// No billing action needed for this milestone in this domain (e.g., transit event).
    NoAction,
}

// ── BillingStrategy trait ─────────────────────────────────────────────────────

#[async_trait]
pub trait BillingStrategy: Send + Sync {
    /// Process a billing milestone and return the outcome.
    ///
    /// Implementations are responsible for:
    ///   1. Creating/updating `Invoice` rows.
    ///   2. Debit/credit on the `DriverLedger`.
    ///   3. Publishing domain-specific Kafka events.
    ///   4. Returning a `MilestoneOutcome` that the caller uses for enforcement.
    async fn process_milestone(
        &self,
        ctx: &MilestoneContext,
    ) -> AppResult<MilestoneOutcome>;

    /// Return the domain identifier — used for logging and metrics tagging.
    fn domain_name(&self) -> &'static str;
}

// ── process_milestone_billing — the unified dispatcher ────────────────────────

/// Route a billing milestone to the correct domain strategy.
///
/// This is the **single entry point** for all billing decisions in the system.
/// Call this from Kafka consumers, HTTP handlers, and hub-ops webhooks alike.
///
/// ```rust,ignore
/// let outcome = process_milestone_billing(&ctx, &balikbayan, &standard, &cod).await?;
/// match outcome {
///     MilestoneOutcome::ShipmentHeld { .. } => return Err(AppError::PaymentRequired(...)),
///     MilestoneOutcome::ManifestCleared { .. } => emit_manifest_gate_cleared(),
///     _ => {}
/// }
/// ```
pub async fn process_milestone_billing(
    ctx:              &MilestoneContext,
    balikbayan_strat: &BalikbayanStrategy,
    standard_strat:   &StandardParcelStrategy,
    cod_strat:        &CodStrategy,
) -> AppResult<MilestoneOutcome> {
    let start = std::time::Instant::now();

    let outcome = match ctx.domain {
        BillingDomain::Balikbayan => {
            tracing::info!(
                shipment_id = %ctx.shipment_id,
                milestone   = ?ctx.milestone,
                domain      = balikbayan_strat.domain_name(),
                "Routing to Balikbayan billing strategy"
            );
            balikbayan_strat.process_milestone(ctx).await?
        }
        BillingDomain::StandardParcel => {
            tracing::info!(
                shipment_id = %ctx.shipment_id,
                milestone   = ?ctx.milestone,
                domain      = standard_strat.domain_name(),
                "Routing to StandardParcel billing strategy"
            );
            standard_strat.process_milestone(ctx).await?
        }
        BillingDomain::Cod => {
            tracing::info!(
                shipment_id = %ctx.shipment_id,
                milestone   = ?ctx.milestone,
                domain      = cod_strat.domain_name(),
                "Routing to COD billing strategy"
            );
            cod_strat.process_milestone(ctx).await?
        }
        BillingDomain::MerchantPeriodic => {
            // Periodic merchant invoices go through the existing BillingAggregationService;
            // they are not milestone-driven and should never reach this dispatcher.
            tracing::warn!(
                shipment_id = %ctx.shipment_id,
                "MerchantPeriodic domain reached milestone dispatcher — no action taken"
            );
            MilestoneOutcome::NoAction
        }
    };

    tracing::info!(
        shipment_id  = %ctx.shipment_id,
        domain       = ctx.domain.as_str(),
        elapsed_ms   = start.elapsed().as_millis(),
        "Billing milestone processed"
    );

    Ok(outcome)
}
