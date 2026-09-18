//! What the routes ask of promotions, composed from the pure rules in
//! `domain` and the ledger in `infrastructure`.

use std::sync::Arc;

use chrono::{DateTime, Datelike, Utc};
use logisticos_errors::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::offer::{eligibility, normalize_code, CodeRefusal, Discount, Offer};
use crate::domain::stack::{stack, Candidate, Candidates, Ceiling, DiscountLine, Gross};
use crate::domain::window::{local_date, month_key, WindowDay, WindowRule};
use crate::infrastructure::db::{NewOffer, NewRedemption, PromotionsStore, RedeemOutcome};

#[derive(Debug, Clone, Copy)]
pub struct Rules {
    pub window: WindowRule,
    pub ceiling_pct: i64,
    /// In the move's own currency units; multiplied to minor units, never converted.
    pub ceiling_flat_units: i64,
    pub utc_offset_minutes: i32,
}

impl Rules {
    fn ceiling(&self) -> Ceiling {
        Ceiling { pct: self.ceiling_pct, flat_cents: self.ceiling_flat_units.max(0).saturating_mul(100) }
    }
}

// ── Views ────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct WindowView {
    /// "2026-09".
    pub month: String,
    pub today: u32,
    pub open_today: bool,
    pub from_day: u32,
    pub to_day: u32,
    pub days: Vec<WindowDay>,
    /// This account has spent this month's code.
    pub used_this_month: bool,
    /// Why a code cannot be used today, when it cannot.
    pub message: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct OfferView {
    pub code: String,
    pub title: String,
    pub body: String,
    pub discount: Discount,
    pub cap_cents: Option<i64>,
    pub windowed: bool,
    pub ends_at: Option<DateTime<Utc>>,
    /// "available" | "used" | "not_now".
    pub state: &'static str,
    pub reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct OffersView {
    pub window: WindowView,
    pub offers: Vec<OfferView>,
}

#[derive(Debug, Serialize)]
pub struct ValidateView {
    pub ok: bool,
    pub code: String,
    pub title: Option<String>,
    pub discount: Option<Discount>,
    pub cap_cents: Option<i64>,
    pub refusal: Option<&'static str>,
    pub message: Option<String>,
}

/// `POST /v1/internal/promotions/price` — order-intake, at quote time.
#[derive(Debug, Clone, Deserialize)]
pub struct PriceRequest {
    pub tenant_id: Uuid,
    pub account_id: Uuid,
    #[serde(default)]
    pub code: Option<String>,
    pub currency: String,
    pub carriage_cents: i64,
    pub accessorial_cents: i64,
}

#[derive(Debug, Serialize)]
pub struct PriceView {
    pub lines: Vec<DiscountLine>,
    pub total_off_cents: i64,
    pub ceiling_cents: i64,
    pub ceiling_binds: bool,
    /// The code, normalised, when it took something off. Only then is it
    /// redeemed at booking.
    pub code_applied: Option<String>,
    pub refusal: Option<&'static str>,
    pub message: Option<String>,
}

/// `POST /v1/internal/promotions/redeem` — order-intake, at booking.
#[derive(Debug, Clone, Deserialize)]
pub struct RedeemRequest {
    pub tenant_id: Uuid,
    pub account_id: Uuid,
    pub shipment_id: Uuid,
    pub code: String,
    pub discount_cents: i64,
    pub currency: String,
}

/// `POST /v1/promotions/admin/offers`.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateOfferRequest {
    pub code: String,
    pub title: String,
    #[serde(default)]
    pub body: String,
    /// "percent_carriage" (with `percent_bps`) or "flat" (with `flat_cents`).
    pub discount_kind: String,
    #[serde(default)]
    pub percent_bps: i64,
    #[serde(default)]
    pub flat_cents: i64,
    #[serde(default)]
    pub cap_cents: Option<i64>,
    #[serde(default = "yes")]
    pub windowed: bool,
    #[serde(default)]
    pub once_per_account: bool,
    #[serde(default)]
    pub starts_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub ends_at: Option<DateTime<Utc>>,
    #[serde(default = "marketing")]
    pub budget_tag: String,
}

fn yes() -> bool { true }
fn marketing() -> String { "marketing".into() }

// ── The service ──────────────────────────────────────────────────────────────

pub struct Promotions {
    store: Arc<dyn PromotionsStore>,
    rules: Rules,
}

impl Promotions {
    pub fn new(store: Arc<dyn PromotionsStore>, rules: Rules) -> Self {
        Self { store, rules }
    }

    /// The offers feed, with each offer's state for this account and the
    /// month grid — the app draws both and decides neither.
    pub async fn offers_for(&self, tenant_id: Uuid, account_id: Uuid, now: DateTime<Utc>) -> AppResult<OffersView> {
        let today = local_date(now, self.rules.utc_offset_minutes);
        let month = month_key(today);
        let history = self.store.history(tenant_id, account_id, &month).await.map_err(AppError::Internal)?;
        let offers = self.store.live_offers(tenant_id, now).await.map_err(AppError::Internal)?;

        let window_check = self.rules.window.check(today);
        let window = WindowView {
            month,
            today: today.day(),
            open_today: window_check.is_ok() && !history.windowed_used_this_month,
            from_day: self.rules.window.from_day,
            to_day: self.rules.window.to_day,
            days: self.rules.window.month(today.year(), today.month()),
            used_this_month: history.windowed_used_this_month,
            message: match window_check {
                Err(w) => Some(w.message()),
                Ok(()) if history.windowed_used_this_month => Some(CodeRefusal::UsedThisMonth.message()),
                Ok(()) => None,
            },
        };

        let offers = offers
            .iter()
            .map(|o| {
                let (state, reason) = if history.offers_used.contains(&o.id) {
                    ("used", None)
                } else {
                    match eligibility(Some(o), now, today, &self.rules.window, &history) {
                        Ok(()) => ("available", None),
                        Err(r) => ("not_now", Some(r.message())),
                    }
                };
                OfferView {
                    code: o.code.clone(),
                    title: o.title.clone(),
                    body: o.body.clone(),
                    discount: o.discount,
                    cap_cents: o.cap_cents,
                    windowed: o.windowed,
                    ends_at: o.ends_at,
                    state,
                    reason,
                }
            })
            .collect();

        Ok(OffersView { window, offers })
    }

    /// May this account use this code today, and if not, which rule said no.
    pub async fn validate(&self, tenant_id: Uuid, account_id: Uuid, code: &str, now: DateTime<Utc>) -> AppResult<ValidateView> {
        let code = normalize_code(code);
        let (offer, verdict) = self.check_code(tenant_id, account_id, &code, now).await?;
        Ok(ValidateView {
            ok: verdict.is_ok(),
            code,
            title: offer.as_ref().map(|o| o.title.clone()),
            discount: offer.as_ref().map(|o| o.discount),
            cap_cents: offer.as_ref().and_then(|o| o.cap_cents),
            refusal: verdict.err().map(|r| r.code()),
            message: verdict.err().map(|r| r.message()),
        })
    }

    /// The discount stack for one quote. A refused code does not fail the
    /// quote: it comes back priced without it, and says why.
    pub async fn price(&self, req: &PriceRequest, now: DateTime<Utc>) -> AppResult<PriceView> {
        let gross = Gross { carriage_cents: req.carriage_cents, accessorial_cents: req.accessorial_cents };
        let mut candidates = Candidates::default();
        let mut refusal = None;

        let code = req.code.as_deref().map(normalize_code).filter(|c| !c.is_empty());
        if let Some(code) = &code {
            let (offer, verdict) = self.check_code(req.tenant_id, req.account_id, code, now).await?;
            match (offer, verdict) {
                (Some(o), Ok(())) => {
                    candidates.code = Some(Candidate { label: o.code.clone(), cents: o.raw_cents(req.carriage_cents) });
                }
                (_, Err(r)) => refusal = Some(r),
                (None, Ok(())) => refusal = Some(CodeRefusal::Unknown),
            }
        }

        let stacked = stack(&gross, &candidates, &self.rules.ceiling());
        let code_applied = stacked
            .lines
            .iter()
            .any(|l| l.kind == crate::domain::stack::LineKind::Code)
            .then(|| code.clone())
            .flatten();

        Ok(PriceView {
            total_off_cents: stacked.total_off_cents,
            ceiling_cents: stacked.ceiling_cents,
            ceiling_binds: stacked.ceiling_binds,
            lines: stacked.lines,
            code_applied,
            refusal: refusal.map(|r| r.code()),
            message: refusal.map(|r| r.message()),
        })
    }

    /// Spend the code on a booking. The ledger's own constraints decide a race
    /// between two bookings; a retry of the same booking is not a second spend.
    pub async fn redeem(&self, req: &RedeemRequest, now: DateTime<Utc>) -> AppResult<()> {
        if req.discount_cents <= 0 {
            return Err(AppError::Validation("A redemption must take something off".into()));
        }
        let code = normalize_code(&req.code);
        let offer = self
            .store
            .find_offer(req.tenant_id, &code)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound { resource: "Offer", id: code.clone() })?;
        let month = month_key(local_date(now, self.rules.utc_offset_minutes));
        let outcome = self
            .store
            .redeem(&NewRedemption {
                tenant_id: req.tenant_id,
                account_id: req.account_id,
                offer,
                shipment_id: req.shipment_id,
                month,
                discount_cents: req.discount_cents,
                currency: req.currency.clone(),
            })
            .await
            .map_err(AppError::Internal)?;
        match outcome {
            RedeemOutcome::Redeemed | RedeemOutcome::AlreadyForThisBooking => Ok(()),
            RedeemOutcome::MonthTaken => Err(AppError::Conflict(CodeRefusal::UsedThisMonth.code().into())),
            RedeemOutcome::OfferTaken => Err(AppError::Conflict(CodeRefusal::AlreadyUsed.code().into())),
        }
    }

    /// The booking was cancelled: the code goes back to the customer.
    pub async fn release(&self, shipment_id: Uuid, now: DateTime<Utc>) -> AppResult<bool> {
        self.store.release(shipment_id, now).await.map_err(AppError::Internal)
    }

    pub async fn list_offers(&self, tenant_id: Uuid) -> AppResult<Vec<Offer>> {
        self.store.list_offers(tenant_id).await.map_err(AppError::Internal)
    }

    pub async fn create_offer(&self, tenant_id: Uuid, req: CreateOfferRequest) -> AppResult<Offer> {
        let code = normalize_code(&req.code);
        if !(3..=32).contains(&code.len()) || !code.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return Err(AppError::Validation("A code is 3–32 letters, digits or hyphens".into()));
        }
        if req.title.trim().is_empty() {
            return Err(AppError::Validation("An offer needs a title".into()));
        }
        let discount = match req.discount_kind.as_str() {
            "percent_carriage" if (1..=10_000).contains(&req.percent_bps) => Discount::PercentCarriage { bps: req.percent_bps },
            "flat" if req.flat_cents > 0 => Discount::Flat { cents: req.flat_cents },
            _ => {
                return Err(AppError::Validation(
                    "discount_kind is percent_carriage (percent_bps 1–10000) or flat (flat_cents > 0)".into(),
                ))
            }
        };
        if req.cap_cents.is_some_and(|c| c <= 0) {
            return Err(AppError::Validation("cap_cents, when set, must be above zero".into()));
        }
        if let (Some(s), Some(e)) = (req.starts_at, req.ends_at) {
            if e <= s {
                return Err(AppError::Validation("ends_at must be after starts_at".into()));
            }
        }
        let budget_tag = req.budget_tag.trim().to_ascii_lowercase();
        if budget_tag.is_empty() || budget_tag.len() > 32 {
            return Err(AppError::Validation("budget_tag names whose budget pays, 1–32 characters".into()));
        }
        self.store
            .create_offer(&NewOffer {
                tenant_id,
                code: code.clone(),
                title: req.title.trim().to_owned(),
                body: req.body.trim().to_owned(),
                discount,
                cap_cents: req.cap_cents,
                windowed: req.windowed,
                once_per_account: req.once_per_account,
                starts_at: req.starts_at,
                ends_at: req.ends_at,
                budget_tag,
            })
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::Conflict(format!("Code {code} already exists")))
    }

    pub async fn set_active(&self, tenant_id: Uuid, offer_id: Uuid, active: bool) -> AppResult<()> {
        if self.store.set_active(tenant_id, offer_id, active).await.map_err(AppError::Internal)? {
            Ok(())
        } else {
            Err(AppError::NotFound { resource: "Offer", id: offer_id.to_string() })
        }
    }

    async fn check_code(
        &self,
        tenant_id: Uuid,
        account_id: Uuid,
        code: &str,
        now: DateTime<Utc>,
    ) -> AppResult<(Option<Offer>, Result<(), CodeRefusal>)> {
        let today = local_date(now, self.rules.utc_offset_minutes);
        let offer = self.store.find_offer(tenant_id, code).await.map_err(AppError::Internal)?;
        let history = self
            .store
            .history(tenant_id, account_id, &month_key(today))
            .await
            .map_err(AppError::Internal)?;
        let verdict = eligibility(offer.as_ref(), now, today, &self.rules.window, &history);
        Ok((offer, verdict))
    }
}

#[cfg(test)]
mod tests;
