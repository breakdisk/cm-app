//! What the routes ask of promotions, composed from the pure rules in
//! `domain` and the stores in `infrastructure`.

use std::sync::Arc;

use chrono::{DateTime, Datelike, Duration, Utc};
use logisticos_errors::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::offer::{eligibility, normalize_code, CodeRefusal, Discount, Offer};
use crate::domain::referral::{may_claim, referral_code};
use crate::domain::stack::{stack, Candidate, Candidates, Ceiling, DiscountLine, Gross, LineKind};
use crate::domain::tier::{next_tier, tier_for, validate_ladder, NewTier, Tier};
use crate::domain::window::{local_date, month_key, WindowDay, WindowRule};
use crate::infrastructure::db::{BookingCommit, BookingLine, CommitOutcome, NewOffer, PromotionsStore};
use crate::infrastructure::rewards_db::{Corporate, CreditEntry, NewCorporate, RewardsStore};

#[derive(Debug, Clone)]
pub struct Rules {
    pub window: WindowRule,
    pub ceiling_pct: i64,
    /// In the move's own currency units; multiplied to minor units, never converted.
    pub ceiling_flat_units: i64,
    pub utc_offset_minutes: i32,
    /// Credit paid to a referrer when their invitee's first move completes,
    /// before the referrer's tier multiplier. 0 turns referral rewards off.
    pub referral_reward_cents: i64,
    /// The currency referral credit is held in. A deployment serves one region.
    pub credit_currency: String,
}

impl Rules {
    fn ceiling(&self) -> Ceiling {
        Ceiling { pct: self.ceiling_pct, flat_cents: self.ceiling_flat_units.max(0).saturating_mul(100) }
    }
}

/// How far back completed moves count toward a tier.
const TIER_WINDOW_DAYS: i64 = 365;

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

#[derive(Debug, Serialize)]
pub struct TierView {
    pub name: String,
    pub min_moves: i32,
    pub perk: String,
    pub accessorial_bps: i64,
    pub cap_cents: i64,
    pub referral_multiplier: i32,
}

impl From<&Tier> for TierView {
    fn from(t: &Tier) -> Self {
        Self {
            name: t.name.clone(),
            min_moves: t.min_moves,
            perk: t.perk.clone(),
            accessorial_bps: t.accessorial_bps,
            cap_cents: t.cap_cents,
            referral_multiplier: t.referral_multiplier,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct LoyaltyView {
    /// False when the tenant's plan does not include loyalty, or it has no ladder.
    pub enabled: bool,
    /// Completed moves in the last 12 months.
    pub moves: i64,
    pub tier: Option<TierView>,
    pub next: Option<TierView>,
    pub moves_to_next: Option<i64>,
    pub ladder: Vec<TierView>,
}

#[derive(Debug, Serialize)]
pub struct CreditView {
    pub balance_cents: i64,
    pub currency: String,
    pub entries: Vec<CreditEntry>,
}

#[derive(Debug, Serialize)]
pub struct InviteeView {
    pub joined_at: DateTime<Utc>,
    /// Their first move has completed and you were credited.
    pub rewarded: bool,
    pub reward_cents: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct ReferralView {
    pub code: String,
    /// What a referral pays you now, your tier included. 0: rewards are off.
    pub reward_cents: i64,
    pub currency: String,
    pub invitees: Vec<InviteeView>,
}

#[derive(Debug, Serialize)]
pub struct ClaimView {
    pub ok: bool,
    pub refusal: Option<&'static str>,
    pub message: Option<&'static str>,
}

#[derive(Debug, Serialize)]
pub struct CorporateRateView {
    pub code: String,
    pub firm_name: String,
    pub percent_bps: i64,
}

impl From<&Corporate> for CorporateRateView {
    fn from(c: &Corporate) -> Self {
        Self { code: c.code.clone(), firm_name: c.firm_name.clone(), percent_bps: c.percent_bps }
    }
}

#[derive(Debug, Serialize)]
pub struct CorporateView {
    pub linked: Option<CorporateRateView>,
    /// A firm whose domain matches this account's email, offered for one-tap
    /// linking. Its code is not shown.
    pub domain_match: Option<String>,
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
    /// From the caller's token: does the tenant's plan include loyalty.
    #[serde(default)]
    pub loyalty_enabled: bool,
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
    /// Credit that did not fit this move and stays on the account.
    pub credit_rollover_cents: i64,
    /// The corporate rate was the larger, so the code is not used or spent.
    pub code_lost_to_corporate: bool,
}

/// One discount line as order-intake signed it into the quote.
#[derive(Debug, Clone, Deserialize)]
pub struct CommitLine {
    pub kind: String,
    pub label: String,
    pub amount_cents: i64,
}

/// `POST /v1/internal/promotions/redeem` — order-intake, at booking: spend
/// the code, apply the credit, record every line.
#[derive(Debug, Clone, Deserialize)]
pub struct CommitRequest {
    pub tenant_id: Uuid,
    pub account_id: Uuid,
    pub shipment_id: Uuid,
    pub currency: String,
    #[serde(default)]
    pub code: Option<String>,
    pub lines: Vec<CommitLine>,
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

/// `POST /v1/promotions/admin/corporate-accounts`.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateCorporateRequest {
    pub code: String,
    pub firm_name: String,
    pub percent_bps: i64,
    #[serde(default)]
    pub email_domain: Option<String>,
}

/// `POST /v1/promotions/admin/credits`.
#[derive(Debug, Clone, Deserialize)]
pub struct GrantCreditRequest {
    pub account_id: Uuid,
    pub amount_cents: i64,
    pub currency: String,
    pub note: String,
}

fn yes() -> bool { true }
fn marketing() -> String { "marketing".into() }

fn valid_code(code: &str) -> bool {
    (3..=32).contains(&code.len()) && code.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn email_domain(email: &str) -> Option<&str> {
    email.rsplit_once('@').map(|(_, d)| d).filter(|d| d.contains('.'))
}

// ── The service ──────────────────────────────────────────────────────────────

pub struct Promotions {
    store: Arc<dyn PromotionsStore>,
    rewards: Arc<dyn RewardsStore>,
    rules: Rules,
}

impl Promotions {
    pub fn new(store: Arc<dyn PromotionsStore>, rewards: Arc<dyn RewardsStore>, rules: Rules) -> Self {
        Self { store, rewards, rules }
    }

    // ── Offers and codes ─────────────────────────────────────────────────────

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

    // ── Pricing and booking ──────────────────────────────────────────────────

    /// The whole discount stack for one quote. A refused code does not fail
    /// the quote: it comes back priced without it, and says why.
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

        // A corporate rate is a tariff on carriage, linked to the account.
        if let Some(corp) = self.rewards.linked_corporate(req.tenant_id, req.account_id).await.map_err(AppError::Internal)? {
            if corp.active {
                candidates.corporate = Some(Candidate {
                    label: corp.firm_name.clone(),
                    cents: req.carriage_cents.max(0) * corp.percent_bps.clamp(0, 10_000) / 10_000,
                });
            }
        }

        // The tier, where the tenant's plan includes loyalty.
        if req.loyalty_enabled {
            let ladder = self.rewards.tiers(req.tenant_id).await.map_err(AppError::Internal)?;
            let moves = self.moves_this_year(req.tenant_id, req.account_id, now).await?;
            if let Some(t) = tier_for(&ladder, moves).filter(|t| t.accessorial_bps > 0) {
                candidates.tier = Some((Candidate { label: t.name.clone(), cents: t.raw_cents(req.accessorial_cents) }, t.cap_cents));
            }
        }

        // Credit in the move's own currency, never converted.
        candidates.credit_cents = self
            .rewards
            .credit_balance(req.tenant_id, req.account_id, &req.currency)
            .await
            .map_err(AppError::Internal)?
            .max(0);

        let stacked = stack(&gross, &candidates, &self.rules.ceiling());
        let code_applied = stacked.lines.iter().any(|l| l.kind == LineKind::Code).then(|| code.clone()).flatten();

        Ok(PriceView {
            total_off_cents: stacked.total_off_cents,
            ceiling_cents: stacked.ceiling_cents,
            ceiling_binds: stacked.ceiling_binds,
            credit_rollover_cents: stacked.credit_rollover_cents,
            code_lost_to_corporate: stacked.code_lost_to_corporate,
            lines: stacked.lines,
            code_applied,
            refusal: refusal.map(|r| r.code()),
            message: refusal.map(|r| r.message()),
        })
    }

    /// Spend what a booking was discounted by, all together or not at all.
    /// The ledger decides a race between two bookings; a retry of the same
    /// booking is not a second spend.
    pub async fn commit(&self, req: &CommitRequest, now: DateTime<Utc>) -> AppResult<()> {
        if req.lines.is_empty() || req.lines.iter().any(|l| l.amount_cents <= 0) {
            return Err(AppError::Validation("A booking commits one or more lines, each taking something off".into()));
        }
        let mut kinds = std::collections::HashSet::new();
        for l in &req.lines {
            if !matches!(l.kind.as_str(), "code" | "corporate" | "tier" | "credit") || !kinds.insert(l.kind.as_str()) {
                return Err(AppError::Validation(format!("Unexpected discount line {:?}", l.kind)));
            }
        }

        let code = match (req.lines.iter().any(|l| l.kind == "code"), req.code.as_deref()) {
            (true, Some(c)) => {
                let c = normalize_code(c);
                let offer = self
                    .store
                    .find_offer(req.tenant_id, &c)
                    .await
                    .map_err(AppError::Internal)?
                    .ok_or_else(|| AppError::NotFound { resource: "Offer", id: c.clone() })?;
                Some((offer, month_key(local_date(now, self.rules.utc_offset_minutes))))
            }
            (true, None) => return Err(AppError::Validation("A code line needs its code".into())),
            (false, _) => None,
        };

        let budget = |kind: &str| -> String {
            match kind {
                "code" => code.as_ref().map(|(o, _)| o.budget_tag.clone()).unwrap_or_else(marketing),
                "corporate" => "corporate".into(),
                "tier" => "loyalty".into(),
                _ => "credit".into(),
            }
        };
        let lines = req
            .lines
            .iter()
            .map(|l| BookingLine { kind: l.kind.clone(), label: l.label.clone(), amount_cents: l.amount_cents, budget_tag: budget(&l.kind) })
            .collect();
        let credit_cents = req.lines.iter().find(|l| l.kind == "credit").map(|l| l.amount_cents).unwrap_or(0);

        let outcome = self
            .store
            .commit_booking(&BookingCommit {
                tenant_id: req.tenant_id,
                account_id: req.account_id,
                shipment_id: req.shipment_id,
                code,
                credit_cents,
                currency: req.currency.clone(),
                lines,
            })
            .await
            .map_err(AppError::Internal)?;
        match outcome {
            CommitOutcome::Committed | CommitOutcome::AlreadyForThisBooking => Ok(()),
            CommitOutcome::MonthTaken => Err(AppError::Conflict(CodeRefusal::UsedThisMonth.code().into())),
            CommitOutcome::OfferTaken => Err(AppError::Conflict(CodeRefusal::AlreadyUsed.code().into())),
            CommitOutcome::CreditShort => Err(AppError::Conflict("CREDIT_CHANGED".into())),
        }
    }

    /// The booking was cancelled, or never came to be: its code goes back and
    /// its credit is returned.
    pub async fn release(&self, shipment_id: Uuid, now: DateTime<Utc>) -> AppResult<bool> {
        self.store.release_booking(shipment_id, now).await.map_err(AppError::Internal)
    }

    // ── Moves, loyalty, referral rewards ─────────────────────────────────────

    pub async fn record_booked(&self, tenant: Uuid, account: Uuid, shipment: Uuid, at: DateTime<Utc>) -> AppResult<()> {
        self.rewards.record_move_booked(tenant, account, shipment, at).await.map_err(AppError::Internal)
    }

    /// A move completed. If it was an invitee's first, their referrer is paid.
    pub async fn record_completed(&self, shipment: Uuid, at: DateTime<Utc>) -> AppResult<()> {
        let Some((tenant, invitee)) = self.rewards.record_move_completed(shipment, at).await.map_err(AppError::Internal)? else {
            return Ok(());
        };
        let Some((referral, referrer)) = self.rewards.unpaid_referral_for(tenant, invitee).await.map_err(AppError::Internal)? else {
            return Ok(());
        };
        let amount = self.referral_reward_for(tenant, referrer, at).await?;
        if self
            .rewards
            .pay_referral(referral, tenant, referrer, amount, &self.rules.credit_currency)
            .await
            .map_err(AppError::Internal)?
        {
            tracing::info!(%tenant, %referrer, %invitee, amount, "referral paid");
        }
        Ok(())
    }

    async fn moves_this_year(&self, tenant: Uuid, account: Uuid, now: DateTime<Utc>) -> AppResult<i64> {
        self.rewards
            .completed_moves_since(tenant, account, now - Duration::days(TIER_WINDOW_DAYS))
            .await
            .map_err(AppError::Internal)
    }

    /// The flat reward, times the referrer's tier multiplier.
    async fn referral_reward_for(&self, tenant: Uuid, referrer: Uuid, now: DateTime<Utc>) -> AppResult<i64> {
        let base = self.rules.referral_reward_cents.max(0);
        if base == 0 {
            return Ok(0);
        }
        let ladder = self.rewards.tiers(tenant).await.map_err(AppError::Internal)?;
        let moves = self.moves_this_year(tenant, referrer, now).await?;
        let multiplier = tier_for(&ladder, moves).map(|t| i64::from(t.referral_multiplier.clamp(1, 5))).unwrap_or(1);
        Ok(base * multiplier)
    }

    pub async fn loyalty(&self, tenant: Uuid, account: Uuid, enabled: bool, now: DateTime<Utc>) -> AppResult<LoyaltyView> {
        let ladder = if enabled { self.rewards.tiers(tenant).await.map_err(AppError::Internal)? } else { Vec::new() };
        let moves = self.moves_this_year(tenant, account, now).await?;
        let tier = tier_for(&ladder, moves);
        let next = next_tier(&ladder, moves);
        Ok(LoyaltyView {
            enabled: enabled && !ladder.is_empty(),
            moves,
            tier: tier.map(TierView::from),
            moves_to_next: next.map(|n| i64::from(n.min_moves) - moves),
            next: next.map(TierView::from),
            ladder: ladder.iter().map(TierView::from).collect(),
        })
    }

    // ── Credit ───────────────────────────────────────────────────────────────

    pub async fn credit(&self, tenant: Uuid, account: Uuid, currency: Option<&str>) -> AppResult<CreditView> {
        let currency = currency.unwrap_or(&self.rules.credit_currency).to_owned();
        Ok(CreditView {
            balance_cents: self.rewards.credit_balance(tenant, account, &currency).await.map_err(AppError::Internal)?.max(0),
            entries: self.rewards.credit_entries(tenant, account, 50).await.map_err(AppError::Internal)?,
            currency,
        })
    }

    pub async fn grant_credit(&self, tenant: Uuid, req: GrantCreditRequest) -> AppResult<()> {
        if req.amount_cents <= 0 {
            return Err(AppError::Validation("A grant adds credit; amount_cents must be above zero".into()));
        }
        let note = req.note.trim();
        if note.is_empty() || note.len() > 200 {
            return Err(AppError::Validation("A grant says why, in 1–200 characters".into()));
        }
        self.rewards
            .grant_credit(tenant, req.account_id, req.amount_cents, req.currency.trim(), note)
            .await
            .map_err(AppError::Internal)
    }

    // ── Referrals ────────────────────────────────────────────────────────────

    /// This account's code (made on first ask) and who joined with it.
    pub async fn referrals(&self, tenant: Uuid, account: Uuid, now: DateTime<Utc>) -> AppResult<ReferralView> {
        let code = self.code_for(tenant, account).await?;
        let invitees = self
            .rewards
            .referrals_by(tenant, account)
            .await
            .map_err(AppError::Internal)?
            .into_iter()
            .map(|r| InviteeView { joined_at: r.joined_at, rewarded: r.rewarded_at.is_some(), reward_cents: r.reward_cents })
            .collect();
        Ok(ReferralView {
            code,
            reward_cents: self.referral_reward_for(tenant, account, now).await?,
            currency: self.rules.credit_currency.clone(),
            invitees,
        })
    }

    async fn code_for(&self, tenant: Uuid, account: Uuid) -> AppResult<String> {
        if let Some(code) = self.rewards.referral_code(tenant, account).await.map_err(AppError::Internal)? {
            return Ok(code);
        }
        // Random codes collide rarely; a few tries settle it.
        for _ in 0..5 {
            let candidate = referral_code(Uuid::new_v4());
            if self.rewards.insert_referral_code(tenant, account, &candidate).await.map_err(AppError::Internal)? {
                return Ok(candidate);
            }
            // Lost a race with this account's own first request?
            if let Some(code) = self.rewards.referral_code(tenant, account).await.map_err(AppError::Internal)? {
                return Ok(code);
            }
        }
        Err(AppError::Internal(anyhow::anyhow!("could not allot a referral code")))
    }

    /// A new account enters a friend's code.
    pub async fn claim_referral(&self, tenant: Uuid, invitee: Uuid, code: &str) -> AppResult<ClaimView> {
        let code = normalize_code(code);
        let referrer = self.rewards.referrer_for_code(tenant, &code).await.map_err(AppError::Internal)?;
        let booked = self.rewards.booked_moves(tenant, invitee).await.map_err(AppError::Internal)?;
        let referred = self.rewards.is_referred(tenant, invitee).await.map_err(AppError::Internal)?;
        let referrer = match may_claim(referrer, invitee, booked, referred) {
            Ok(r) => r,
            Err(refusal) => return Ok(ClaimView { ok: false, refusal: Some(refusal.code()), message: Some(refusal.message()) }),
        };
        if !self.rewards.insert_referral(tenant, referrer, invitee).await.map_err(AppError::Internal)? {
            let r = crate::domain::referral::ClaimRefusal::AlreadyReferred;
            return Ok(ClaimView { ok: false, refusal: Some(r.code()), message: Some(r.message()) });
        }
        Ok(ClaimView { ok: true, refusal: None, message: None })
    }

    // ── Corporate rates ──────────────────────────────────────────────────────

    pub async fn corporate(&self, tenant: Uuid, account: Uuid, email: &str) -> AppResult<CorporateView> {
        let linked = self.rewards.linked_corporate(tenant, account).await.map_err(AppError::Internal)?;
        let domain_match = match (&linked, email_domain(email)) {
            (None, Some(d)) => self.rewards.corporate_by_domain(tenant, d).await.map_err(AppError::Internal)?.map(|c| c.firm_name),
            _ => None,
        };
        Ok(CorporateView { linked: linked.as_ref().filter(|c| c.active).map(CorporateRateView::from), domain_match })
    }

    /// Link by code, or — with no code — to the firm matching this account's
    /// work email.
    pub async fn link_corporate(&self, tenant: Uuid, account: Uuid, code: Option<&str>, email: &str) -> AppResult<CorporateView> {
        let corp = match code.map(normalize_code).filter(|c| !c.is_empty()) {
            Some(c) => self.rewards.corporate_by_code(tenant, &c).await.map_err(AppError::Internal)?,
            None => match email_domain(email) {
                Some(d) => self.rewards.corporate_by_domain(tenant, d).await.map_err(AppError::Internal)?,
                None => None,
            },
        };
        let corp = corp
            .filter(|c| c.active)
            .ok_or_else(|| AppError::NotFound { resource: "Company code", id: code.unwrap_or_default().to_owned() })?;
        self.rewards.link_corporate(tenant, account, corp.id).await.map_err(AppError::Internal)?;
        self.corporate(tenant, account, email).await
    }

    pub async fn unlink_corporate(&self, tenant: Uuid, account: Uuid) -> AppResult<()> {
        self.rewards.unlink_corporate(tenant, account).await.map_err(AppError::Internal).map(|_| ())
    }

    // ── Admin ────────────────────────────────────────────────────────────────

    pub async fn list_offers(&self, tenant_id: Uuid) -> AppResult<Vec<Offer>> {
        self.store.list_offers(tenant_id).await.map_err(AppError::Internal)
    }

    pub async fn create_offer(&self, tenant_id: Uuid, req: CreateOfferRequest) -> AppResult<Offer> {
        let code = normalize_code(&req.code);
        if !valid_code(&code) {
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

    pub async fn replace_ladder(&self, tenant: Uuid, tiers: Vec<NewTier>) -> AppResult<Vec<TierView>> {
        validate_ladder(&tiers).map_err(AppError::Validation)?;
        let saved = self.rewards.replace_tiers(tenant, &tiers).await.map_err(AppError::Internal)?;
        Ok(saved.iter().map(TierView::from).collect())
    }

    pub async fn list_corporates(&self, tenant: Uuid) -> AppResult<Vec<Corporate>> {
        self.rewards.list_corporates(tenant).await.map_err(AppError::Internal)
    }

    pub async fn create_corporate(&self, tenant: Uuid, req: CreateCorporateRequest) -> AppResult<Corporate> {
        let code = normalize_code(&req.code);
        if !valid_code(&code) {
            return Err(AppError::Validation("A company code is 3–32 letters, digits or hyphens".into()));
        }
        if req.firm_name.trim().is_empty() {
            return Err(AppError::Validation("A company needs a name".into()));
        }
        if !(1..=10_000).contains(&req.percent_bps) {
            return Err(AppError::Validation("percent_bps is 1–10000".into()));
        }
        let email_domain = req.email_domain.map(|d| d.trim().trim_start_matches('@').to_ascii_lowercase()).filter(|d| !d.is_empty());
        if email_domain.as_deref().is_some_and(|d| !d.contains('.')) {
            return Err(AppError::Validation("email_domain looks like example.com".into()));
        }
        self.rewards
            .create_corporate(
                tenant,
                &NewCorporate { code: code.clone(), firm_name: req.firm_name.trim().to_owned(), percent_bps: req.percent_bps, email_domain },
            )
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::Conflict(format!("Company code {code} already exists")))
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
