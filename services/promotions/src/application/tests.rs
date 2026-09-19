use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use super::*;
use crate::domain::offer::AccountHistory;
use crate::infrastructure::rewards_db::ReferralRow;

// ── A fake with the ledger's rules, as the migrations' indexes enforce them ──

#[derive(Clone)]
struct Redemption {
    tenant: Uuid,
    account: Uuid,
    offer: Offer,
    shipment: Uuid,
    month: String,
    released: bool,
}

#[derive(Clone)]
struct Credit {
    tenant: Uuid,
    account: Uuid,
    amount: i64,
    currency: String,
    kind: &'static str,
    shipment: Option<Uuid>,
}

/// shipment, tenant, account, completed
type MoveRow = (Uuid, Uuid, Uuid, Option<DateTime<Utc>>);
/// id, tenant, referrer, invitee, paid
type ReferralFake = (Uuid, Uuid, Uuid, Uuid, Option<i64>);

#[derive(Default)]
struct Fake {
    offers: Mutex<Vec<Offer>>,
    redemptions: Mutex<Vec<Redemption>>,
    committed: Mutex<Vec<Uuid>>,
    credits: Mutex<Vec<Credit>>,
    tiers: Mutex<Vec<Tier>>,
    moves: Mutex<Vec<MoveRow>>,
    codes: Mutex<Vec<(Uuid, Uuid, String)>>,
    referrals: Mutex<Vec<ReferralFake>>,
    corporates: Mutex<Vec<Corporate>>,
    links: Mutex<Vec<(Uuid, Uuid, Uuid)>>,
}

impl Fake {
    fn balance(&self, tenant: Uuid, account: Uuid, currency: &str) -> i64 {
        self.credits
            .lock()
            .unwrap()
            .iter()
            .filter(|c| c.tenant == tenant && c.account == account && c.currency == currency)
            .map(|c| c.amount)
            .sum()
    }
}

#[async_trait]
impl PromotionsStore for Fake {
    async fn find_offer(&self, tenant_id: Uuid, code: &str) -> anyhow::Result<Option<Offer>> {
        Ok(self.offers.lock().unwrap().iter().find(|o| o.tenant_id == tenant_id && o.code == code).cloned())
    }
    async fn live_offers(&self, tenant_id: Uuid, now: DateTime<Utc>) -> anyhow::Result<Vec<Offer>> {
        Ok(self.offers.lock().unwrap().iter().filter(|o| o.tenant_id == tenant_id && o.live_at(now)).cloned().collect())
    }
    async fn history(&self, tenant_id: Uuid, account_id: Uuid, month: &str) -> anyhow::Result<AccountHistory> {
        let mut h = AccountHistory::default();
        for r in self.redemptions.lock().unwrap().iter() {
            if r.tenant == tenant_id && r.account == account_id && !r.released {
                if r.offer.windowed && r.month == month {
                    h.windowed_used_this_month = true;
                }
                h.offers_used.push(r.offer.id);
            }
        }
        Ok(h)
    }
    async fn commit_booking(&self, c: &BookingCommit) -> anyhow::Result<CommitOutcome> {
        if self.committed.lock().unwrap().contains(&c.shipment_id) {
            return Ok(CommitOutcome::AlreadyForThisBooking);
        }
        if let Some((offer, month)) = &c.code {
            let reds = self.redemptions.lock().unwrap();
            let standing = |r: &&Redemption| !r.released && r.tenant == c.tenant_id && r.account == c.account_id;
            if offer.windowed && reds.iter().filter(standing).any(|r| r.offer.windowed && &r.month == month) {
                return Ok(CommitOutcome::MonthTaken);
            }
            if offer.once_per_account && reds.iter().filter(standing).any(|r| r.offer.id == offer.id) {
                return Ok(CommitOutcome::OfferTaken);
            }
        }
        if c.credit_cents > 0 && self.balance(c.tenant_id, c.account_id, &c.currency) < c.credit_cents {
            return Ok(CommitOutcome::CreditShort);
        }
        if let Some((offer, month)) = &c.code {
            self.redemptions.lock().unwrap().push(Redemption {
                tenant: c.tenant_id,
                account: c.account_id,
                offer: offer.clone(),
                shipment: c.shipment_id,
                month: month.clone(),
                released: false,
            });
        }
        if c.credit_cents > 0 {
            self.credits.lock().unwrap().push(Credit {
                tenant: c.tenant_id,
                account: c.account_id,
                amount: -c.credit_cents,
                currency: c.currency.clone(),
                kind: "applied",
                shipment: Some(c.shipment_id),
            });
        }
        self.committed.lock().unwrap().push(c.shipment_id);
        Ok(CommitOutcome::Committed)
    }
    async fn release_booking(&self, shipment_id: Uuid, _at: DateTime<Utc>) -> anyhow::Result<bool> {
        let mut changed = false;
        for r in self.redemptions.lock().unwrap().iter_mut().filter(|r| r.shipment == shipment_id && !r.released) {
            r.released = true;
            changed = true;
        }
        let mut credits = self.credits.lock().unwrap();
        let returned = credits.iter().any(|c| c.shipment == Some(shipment_id) && c.kind == "returned");
        if let Some(applied) = credits.iter().find(|c| c.shipment == Some(shipment_id) && c.kind == "applied").cloned() {
            if !returned {
                credits.push(Credit { amount: -applied.amount, kind: "returned", ..applied });
                changed = true;
            }
        }
        Ok(changed)
    }
    async fn list_offers(&self, tenant_id: Uuid) -> anyhow::Result<Vec<Offer>> {
        self.live_offers(tenant_id, Utc::now()).await
    }
    async fn create_offer(&self, _o: &NewOffer) -> anyhow::Result<Option<Offer>> {
        Ok(None)
    }
    async fn set_active(&self, _t: Uuid, _o: Uuid, _a: bool) -> anyhow::Result<bool> {
        Ok(false)
    }
}

#[async_trait]
impl RewardsStore for Fake {
    async fn set_corporate_active(&self, tenant: Uuid, id: Uuid, active: bool) -> anyhow::Result<bool> {
        let _ = tenant;
        match self.corporates.lock().unwrap().iter_mut().find(|c| c.id == id) {
            Some(c) => {
                c.active = active;
                Ok(true)
            }
            None => Ok(false),
        }
    }
    async fn tiers(&self, _tenant: Uuid) -> anyhow::Result<Vec<Tier>> {
        Ok(self.tiers.lock().unwrap().clone())
    }
    async fn completed_moves_since(&self, tenant: Uuid, account: Uuid, since: DateTime<Utc>) -> anyhow::Result<i64> {
        Ok(self.moves.lock().unwrap().iter().filter(|m| m.1 == tenant && m.2 == account && m.3.is_some_and(|t| t >= since)).count() as i64)
    }
    async fn booked_moves(&self, tenant: Uuid, account: Uuid) -> anyhow::Result<i64> {
        Ok(self.moves.lock().unwrap().iter().filter(|m| m.1 == tenant && m.2 == account).count() as i64)
    }
    async fn record_move_booked(&self, tenant: Uuid, account: Uuid, shipment: Uuid, _at: DateTime<Utc>) -> anyhow::Result<()> {
        let mut moves = self.moves.lock().unwrap();
        if !moves.iter().any(|m| m.0 == shipment) {
            moves.push((shipment, tenant, account, None));
        }
        Ok(())
    }
    async fn record_move_completed(&self, shipment: Uuid, at: DateTime<Utc>) -> anyhow::Result<Option<(Uuid, Uuid)>> {
        let mut moves = self.moves.lock().unwrap();
        match moves.iter_mut().find(|m| m.0 == shipment && m.3.is_none()) {
            Some(m) => {
                m.3 = Some(at);
                Ok(Some((m.1, m.2)))
            }
            None => Ok(None),
        }
    }
    async fn credit_balance(&self, tenant: Uuid, account: Uuid, currency: &str) -> anyhow::Result<i64> {
        Ok(self.balance(tenant, account, currency))
    }
    async fn referral_code(&self, tenant: Uuid, account: Uuid) -> anyhow::Result<Option<String>> {
        Ok(self.codes.lock().unwrap().iter().find(|c| c.0 == tenant && c.1 == account).map(|c| c.2.clone()))
    }
    async fn insert_referral_code(&self, tenant: Uuid, account: Uuid, code: &str) -> anyhow::Result<bool> {
        let mut codes = self.codes.lock().unwrap();
        if codes.iter().any(|c| c.0 == tenant && (c.1 == account || c.2 == code)) {
            return Ok(false);
        }
        codes.push((tenant, account, code.to_owned()));
        Ok(true)
    }
    async fn referrer_for_code(&self, tenant: Uuid, code: &str) -> anyhow::Result<Option<Uuid>> {
        Ok(self.codes.lock().unwrap().iter().find(|c| c.0 == tenant && c.2 == code).map(|c| c.1))
    }
    async fn is_referred(&self, tenant: Uuid, invitee: Uuid) -> anyhow::Result<bool> {
        Ok(self.referrals.lock().unwrap().iter().any(|r| r.1 == tenant && r.3 == invitee))
    }
    async fn insert_referral(&self, tenant: Uuid, referrer: Uuid, invitee: Uuid) -> anyhow::Result<bool> {
        let mut refs = self.referrals.lock().unwrap();
        if refs.iter().any(|r| r.1 == tenant && r.3 == invitee) {
            return Ok(false);
        }
        refs.push((Uuid::new_v4(), tenant, referrer, invitee, None));
        Ok(true)
    }
    async fn referrals_by(&self, tenant: Uuid, referrer: Uuid) -> anyhow::Result<Vec<ReferralRow>> {
        Ok(self
            .referrals
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.1 == tenant && r.2 == referrer)
            .map(|r| ReferralRow { id: r.0, joined_at: Utc::now(), rewarded_at: r.4.map(|_| Utc::now()), reward_cents: r.4 })
            .collect())
    }
    async fn unpaid_referral_for(&self, tenant: Uuid, invitee: Uuid) -> anyhow::Result<Option<(Uuid, Uuid)>> {
        Ok(self.referrals.lock().unwrap().iter().find(|r| r.1 == tenant && r.3 == invitee && r.4.is_none()).map(|r| (r.0, r.2)))
    }
    async fn pay_referral(&self, referral: Uuid, tenant: Uuid, referrer: Uuid, amount: i64, currency: &str) -> anyhow::Result<bool> {
        let mut refs = self.referrals.lock().unwrap();
        let Some(r) = refs.iter_mut().find(|r| r.0 == referral && r.4.is_none()) else {
            return Ok(false);
        };
        r.4 = Some(amount);
        if amount > 0 {
            self.credits.lock().unwrap().push(Credit {
                tenant,
                account: referrer,
                amount,
                currency: currency.into(),
                kind: "referral_reward",
                shipment: None,
            });
        }
        Ok(true)
    }
    async fn linked_corporate(&self, tenant: Uuid, account: Uuid) -> anyhow::Result<Option<Corporate>> {
        let links = self.links.lock().unwrap();
        let Some(link) = links.iter().find(|l| l.0 == tenant && l.1 == account) else {
            return Ok(None);
        };
        Ok(self.corporates.lock().unwrap().iter().find(|c| c.id == link.2).cloned())
    }
}

// ── Fixtures ─────────────────────────────────────────────────────────────────

const TENANT: Uuid = Uuid::from_u128(1);
const ME: Uuid = Uuid::from_u128(2);
const FRIEND: Uuid = Uuid::from_u128(3);

fn rules(reward: i64) -> Rules {
    Rules {
        window: WindowRule { from_day: 11, to_day: 24 },
        ceiling_pct: 20,
        ceiling_flat_units: 50,
        utc_offset_minutes: 480,
        referral_reward_cents: reward,
        credit_currency: "PHP".into(),
    }
}

/// Tuesday 15 September 2026, 11:00 in Manila.
fn tuesday() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-09-15T03:00:00Z").unwrap().with_timezone(&Utc)
}

fn move20() -> Offer {
    Offer {
        id: Uuid::from_u128(20),
        tenant_id: TENANT,
        code: "MOVE20".into(),
        title: "Autumn move-in".into(),
        body: String::new(),
        discount: Discount::PercentCarriage { bps: 2_000 },
        cap_cents: Some(2_500),
        windowed: true,
        once_per_account: false,
        starts_at: None,
        ends_at: None,
        active: true,
        budget_tag: "marketing".into(),
    }
}

fn silver() -> Tier {
    Tier { id: Uuid::new_v4(), name: "Silver".into(), min_moves: 2, perk: String::new(), accessorial_bps: 1_500, cap_cents: 2_000, referral_multiplier: 2 }
}

fn setup(fake: Fake, reward: i64) -> (Promotions, Arc<Fake>) {
    let fake = Arc::new(fake);
    (Promotions::new(fake.clone(), fake.clone(), rules(reward)), fake)
}

fn service(offers: Vec<Offer>) -> Promotions {
    setup(Fake { offers: Mutex::new(offers), ..Default::default() }, 0).0
}

fn quote(code: Option<&str>) -> PriceRequest {
    PriceRequest {
        tenant_id: TENANT,
        account_id: ME,
        code: code.map(str::to_owned),
        currency: "PHP".into(),
        carriage_cents: 30_000,
        accessorial_cents: 5_000,
        loyalty_enabled: false,
    }
}

fn line(kind: &str, amount: i64) -> CommitLine {
    CommitLine { kind: kind.into(), label: kind.into(), amount_cents: amount }
}

fn commit(shipment: u128, code: Option<&str>, lines: Vec<CommitLine>) -> CommitRequest {
    CommitRequest {
        tenant_id: TENANT,
        account_id: ME,
        shipment_id: Uuid::from_u128(shipment),
        currency: "PHP".into(),
        code: code.map(str::to_owned),
        lines,
    }
}

fn code_booking(shipment: u128) -> CommitRequest {
    commit(shipment, Some("move20"), vec![line("code", 2_500)])
}

// ── Codes (D1) ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_valid_code_comes_off_the_quote() {
    let p = service(vec![move20()]);
    let v = p.price(&quote(Some("move20")), tuesday()).await.unwrap();
    assert_eq!(v.total_off_cents, 2_500);
    assert_eq!(v.code_applied.as_deref(), Some("MOVE20"));
    assert!(v.refusal.is_none());
}

#[tokio::test]
async fn a_refused_code_prices_without_it_and_says_why() {
    let p = service(vec![move20()]);
    let saturday = DateTime::parse_from_rfc3339("2026-09-19T03:00:00Z").unwrap().with_timezone(&Utc);
    let v = p.price(&quote(Some("MOVE20")), saturday).await.unwrap();
    assert_eq!(v.total_off_cents, 0);
    assert_eq!(v.code_applied, None);
    assert_eq!(v.refusal, Some("OUTSIDE_WINDOW_WEEKEND"));
}

#[tokio::test]
async fn no_code_is_no_discount_and_no_refusal() {
    let p = service(vec![move20()]);
    let v = p.price(&quote(None), tuesday()).await.unwrap();
    assert_eq!(v.total_off_cents, 0);
    assert!(v.refusal.is_none());
}

/// Two quotes, two bookings, one month: the second loses at the ledger.
#[tokio::test]
async fn the_second_booking_in_a_month_is_refused_at_redemption() {
    let p = service(vec![move20()]);
    p.commit(&code_booking(100), tuesday()).await.unwrap();
    let second = p.commit(&code_booking(101), tuesday()).await;
    assert!(matches!(second, Err(AppError::Conflict(ref c)) if c == "ALREADY_REDEEMED_THIS_MONTH"));
}

#[tokio::test]
async fn a_retried_booking_is_not_a_second_spend() {
    let p = service(vec![move20()]);
    p.commit(&code_booking(100), tuesday()).await.unwrap();
    p.commit(&code_booking(100), tuesday()).await.unwrap();
}

/// Cancelled: the code goes back, and the month is open again.
#[tokio::test]
async fn a_cancelled_booking_gives_the_code_back() {
    let p = service(vec![move20()]);
    p.commit(&code_booking(100), tuesday()).await.unwrap();
    let after = p.price(&quote(Some("MOVE20")), tuesday()).await.unwrap();
    assert_eq!(after.refusal, Some("ALREADY_REDEEMED_THIS_MONTH"));

    assert!(p.release(Uuid::from_u128(100), tuesday()).await.unwrap());
    let again = p.price(&quote(Some("MOVE20")), tuesday()).await.unwrap();
    assert_eq!(again.total_off_cents, 2_500);
}

#[tokio::test]
async fn the_feed_marks_what_this_account_has_used() {
    let mut welcome = move20();
    welcome.id = Uuid::from_u128(15);
    welcome.code = "WELCOME15".into();
    welcome.windowed = false;
    welcome.once_per_account = true;
    welcome.discount = Discount::Flat { cents: 1_500 };
    let p = service(vec![move20(), welcome]);
    p.commit(&commit(100, Some("WELCOME15"), vec![line("code", 1_500)]), tuesday()).await.unwrap();

    let feed = p.offers_for(TENANT, ME, tuesday()).await.unwrap();
    let state = |c: &str| feed.offers.iter().find(|o| o.code == c).unwrap().state;
    assert_eq!(state("WELCOME15"), "used");
    assert_eq!(state("MOVE20"), "available", "a once-per-account code does not spend the month");
    assert!(feed.window.open_today);
    assert_eq!(feed.window.month, "2026-09");
}

#[tokio::test]
async fn an_unknown_code_is_refused_by_name() {
    let p = service(vec![]);
    let v = p.validate(TENANT, ME, "nope", tuesday()).await.unwrap();
    assert!(!v.ok);
    assert_eq!(v.refusal, Some("UNKNOWN_CODE"));
}

#[tokio::test]
async fn a_commit_names_only_known_lines() {
    let p = service(vec![move20()]);
    let bad = p.commit(&commit(100, None, vec![line("mystery", 100)]), tuesday()).await;
    assert!(matches!(bad, Err(AppError::Validation(_))));
    let no_code = p.commit(&commit(100, None, vec![line("code", 100)]), tuesday()).await;
    assert!(matches!(no_code, Err(AppError::Validation(_))), "a code line without its code");
}

// ── Loyalty ──────────────────────────────────────────────────────────────────

fn completed_moves(fake: &Fake, account: Uuid, n: usize, at: DateTime<Utc>) {
    for i in 0..n {
        fake.moves.lock().unwrap().push((Uuid::new_v4(), TENANT, account, Some(at - Duration::days(i as i64))));
    }
}

#[tokio::test]
async fn a_tier_takes_its_share_of_accessorials_within_its_cap() {
    let (p, fake) = setup(Fake { tiers: Mutex::new(vec![silver()]), ..Default::default() }, 0);
    completed_moves(&fake, ME, 3, tuesday());

    let v = p.price(&PriceRequest { loyalty_enabled: true, ..quote(None) }, tuesday()).await.unwrap();
    // 15% of 50.00 accessorials is 7.50, under the 20.00 cap.
    assert_eq!(v.total_off_cents, 750);
    assert_eq!(v.lines[0].kind, LineKind::Tier);
}

#[tokio::test]
async fn no_tier_discount_where_the_plan_has_no_loyalty() {
    let (p, fake) = setup(Fake { tiers: Mutex::new(vec![silver()]), ..Default::default() }, 0);
    completed_moves(&fake, ME, 3, tuesday());
    let v = p.price(&quote(None), tuesday()).await.unwrap();
    assert_eq!(v.total_off_cents, 0);
    let view = p.loyalty(TENANT, ME, false, tuesday()).await.unwrap();
    assert!(!view.enabled);
}

/// Moves older than a year do not count toward the tier.
#[tokio::test]
async fn the_tier_counts_the_last_twelve_months() {
    let (p, fake) = setup(Fake { tiers: Mutex::new(vec![silver()]), ..Default::default() }, 0);
    completed_moves(&fake, ME, 1, tuesday());
    completed_moves(&fake, ME, 1, tuesday() - Duration::days(400));
    let view = p.loyalty(TENANT, ME, true, tuesday()).await.unwrap();
    assert_eq!(view.moves, 1);
    assert!(view.tier.is_none());
    assert_eq!(view.moves_to_next, Some(1));
}

// ── Corporate ────────────────────────────────────────────────────────────────

fn acme() -> Corporate {
    Corporate { id: Uuid::from_u128(77), code: "ACME-FRT".into(), firm_name: "Acme Logistics".into(), percent_bps: 1_200, email_domain: None, active: true }
}

/// Acme's 12% of 300.00 is 36.00; MOVE20 is 25.00. The larger wins and the
/// code is neither applied nor spent.
#[tokio::test]
async fn a_larger_corporate_rate_beats_the_code_and_saves_it() {
    let fake = Fake {
        offers: Mutex::new(vec![move20()]),
        corporates: Mutex::new(vec![acme()]),
        links: Mutex::new(vec![(TENANT, ME, Uuid::from_u128(77))]),
        ..Default::default()
    };
    let (p, _) = setup(fake, 0);
    let v = p.price(&quote(Some("MOVE20")), tuesday()).await.unwrap();
    assert_eq!(v.total_off_cents, 3_600);
    assert!(v.code_lost_to_corporate);
    assert_eq!(v.code_applied, None);
}

// ── Credit ───────────────────────────────────────────────────────────────────

fn with_credit(amount: i64) -> Fake {
    let fake = Fake::default();
    fake.credits.lock().unwrap().push(Credit {
        tenant: TENANT,
        account: ME,
        amount,
        currency: "PHP".into(),
        kind: "grant",
        shipment: None,
    });
    fake
}

#[tokio::test]
async fn credit_comes_off_and_the_rest_rolls_over() {
    let (p, _) = setup(with_credit(6_000), 0);
    let v = p.price(&quote(None), tuesday()).await.unwrap();
    // The ceiling is 50.00: 20% of 350.00 is 70.00, the flat 50 binds.
    assert_eq!(v.total_off_cents, 5_000);
    assert_eq!(v.credit_rollover_cents, 1_000);
}

#[tokio::test]
async fn credit_in_another_currency_is_not_spent() {
    let (p, _) = setup(with_credit(6_000), 0);
    let v = p.price(&PriceRequest { currency: "AED".into(), ..quote(None) }, tuesday()).await.unwrap();
    assert_eq!(v.total_off_cents, 0);
}

/// Spent on another booking since the quote: this one must not overdraw.
#[tokio::test]
async fn credit_spent_since_the_quote_stops_the_booking() {
    let (p, _) = setup(with_credit(1_000), 0);
    p.commit(&commit(100, None, vec![line("credit", 1_000)]), tuesday()).await.unwrap();
    let second = p.commit(&commit(101, None, vec![line("credit", 1_000)]), tuesday()).await;
    assert!(matches!(second, Err(AppError::Conflict(ref c)) if c == "CREDIT_CHANGED"));
}

#[tokio::test]
async fn a_cancelled_booking_returns_its_credit_once() {
    let (p, fake) = setup(with_credit(1_000), 0);
    p.commit(&commit(100, None, vec![line("credit", 1_000)]), tuesday()).await.unwrap();
    assert_eq!(fake.balance(TENANT, ME, "PHP"), 0);
    p.release(Uuid::from_u128(100), tuesday()).await.unwrap();
    p.release(Uuid::from_u128(100), tuesday()).await.unwrap();
    assert_eq!(fake.balance(TENANT, ME, "PHP"), 1_000);
}

// ── Referrals ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn an_accounts_code_is_the_same_every_time() {
    let (p, _) = setup(Fake::default(), 1_500);
    let a = p.referrals(TENANT, ME, tuesday()).await.unwrap().code;
    let b = p.referrals(TENANT, ME, tuesday()).await.unwrap().code;
    assert_eq!(a, b);
}

/// FRIEND joins with my code; their first completed move pays me, doubled by
/// my Silver tier. Their second move pays nothing more.
#[tokio::test]
async fn a_friends_first_completed_move_pays_the_referrer_once() {
    let (p, fake) = setup(Fake { tiers: Mutex::new(vec![silver()]), ..Default::default() }, 1_500);
    completed_moves(&fake, ME, 2, tuesday());
    let code = p.referrals(TENANT, ME, tuesday()).await.unwrap().code;

    let claim = p.claim_referral(TENANT, FRIEND, &code).await.unwrap();
    assert!(claim.ok);

    let (first, second) = (Uuid::new_v4(), Uuid::new_v4());
    p.record_booked(TENANT, FRIEND, first, tuesday()).await.unwrap();
    p.record_booked(TENANT, FRIEND, second, tuesday()).await.unwrap();
    p.record_completed(first, tuesday(), tuesday()).await.unwrap();
    p.record_completed(second, tuesday(), tuesday()).await.unwrap();

    assert_eq!(fake.balance(TENANT, ME, "PHP"), 3_000);
}

#[tokio::test]
async fn an_account_that_has_booked_cannot_be_referred() {
    let (p, _) = setup(Fake::default(), 1_500);
    let code = p.referrals(TENANT, ME, tuesday()).await.unwrap().code;
    p.record_booked(TENANT, FRIEND, Uuid::new_v4(), tuesday()).await.unwrap();
    let claim = p.claim_referral(TENANT, FRIEND, &code).await.unwrap();
    assert_eq!(claim.refusal, Some("NOT_A_NEW_ACCOUNT"));
}

#[tokio::test]
async fn referral_rewards_off_pay_nothing_but_still_record_the_join() {
    let (p, fake) = setup(Fake::default(), 0);
    let code = p.referrals(TENANT, ME, tuesday()).await.unwrap().code;
    p.claim_referral(TENANT, FRIEND, &code).await.unwrap();
    let shipment = Uuid::new_v4();
    p.record_booked(TENANT, FRIEND, shipment, tuesday()).await.unwrap();
    p.record_completed(shipment, tuesday(), tuesday()).await.unwrap();
    assert_eq!(fake.balance(TENANT, ME, "PHP"), 0);
    assert_eq!(p.referrals(TENANT, ME, tuesday()).await.unwrap().invitees.len(), 1);
}

// ── Tier upgrades ────────────────────────────────────────────────────────────

#[derive(Default)]
struct Announced(Mutex<Vec<logisticos_events::payloads::TierChanged>>);

#[async_trait]
impl TierEvents for Announced {
    async fn tier_changed(&self, e: &logisticos_events::payloads::TierChanged) -> anyhow::Result<()> {
        self.0.lock().unwrap().push(e.clone());
        Ok(())
    }
}

fn with_ladder() -> (Promotions, Arc<Fake>, Arc<Announced>) {
    let bronze = Tier { id: Uuid::new_v4(), name: "Bronze".into(), min_moves: 1, perk: "Priority support".into(), accessorial_bps: 0, cap_cents: 0, referral_multiplier: 1 };
    let fake = Arc::new(Fake { tiers: Mutex::new(vec![bronze, silver()]), ..Default::default() });
    let announced = Arc::new(Announced::default());
    let p = Promotions::new(fake.clone(), fake.clone(), rules(0)).with_tier_events(announced.clone());
    (p, fake, announced)
}

async fn complete_one(p: &Promotions, at: DateTime<Utc>, now: DateTime<Utc>) {
    let shipment = Uuid::new_v4();
    p.record_booked(TENANT, ME, shipment, at).await.unwrap();
    p.record_completed(shipment, at, now).await.unwrap();
}

#[tokio::test]
async fn crossing_a_rung_is_announced_once_with_what_it_gives() {
    let (p, _, announced) = with_ladder();
    complete_one(&p, tuesday(), tuesday()).await; // 1 → Bronze
    complete_one(&p, tuesday(), tuesday()).await; // 2 → Silver
    complete_one(&p, tuesday(), tuesday()).await; // 3 → still Silver

    let got = announced.0.lock().unwrap().clone();
    assert_eq!(got.len(), 2);
    assert_eq!((got[0].tier_name.as_str(), got[0].previous_tier.as_deref()), ("Bronze", None));
    assert_eq!(got[0].perk, "Priority support");
    assert_eq!((got[1].tier_name.as_str(), got[1].previous_tier.as_deref()), ("Silver", Some("Bronze")));
    assert_eq!(got[1].perk, "15% off extras on every move");
    assert_eq!(got[1].account_id, ME);
}

#[tokio::test]
async fn a_replayed_old_completion_counts_but_is_not_pushed() {
    let (p, _, announced) = with_ladder();
    complete_one(&p, tuesday(), tuesday() + Duration::days(3)).await;
    assert!(announced.0.lock().unwrap().is_empty());
    assert_eq!(p.loyalty(TENANT, ME, true, tuesday()).await.unwrap().tier.map(|t| t.name).as_deref(), Some("Bronze"));
}

#[tokio::test]
async fn a_redelivered_completion_announces_nothing_new() {
    let (p, _, announced) = with_ladder();
    let shipment = Uuid::new_v4();
    p.record_booked(TENANT, ME, shipment, tuesday()).await.unwrap();
    p.record_completed(shipment, tuesday(), tuesday()).await.unwrap();
    p.record_completed(shipment, tuesday(), tuesday()).await.unwrap();
    assert_eq!(announced.0.lock().unwrap().len(), 1);
}

#[test]
fn a_fractional_discount_reads_without_trailing_zeros() {
    let t = Tier { accessorial_bps: 1_250, ..silver() };
    assert_eq!(tier_perk_text(&t), "12.5% off extras on every move");
}

#[tokio::test]
async fn a_company_rate_switched_off_stops_applying_and_back_on_restores_it() {
    let (p, fake) = setup(Fake { corporates: Mutex::new(vec![acme()]), ..Default::default() }, 0);
    let id = fake.corporates.lock().unwrap()[0].id;
    fake.links.lock().unwrap().push((TENANT, ME, id));

    p.set_corporate_active(TENANT, id, false).await.unwrap();
    let off = p.price(&quote(None), tuesday()).await.unwrap();
    assert!(off.lines.iter().all(|l| l.kind != LineKind::Corporate));

    p.set_corporate_active(TENANT, id, true).await.unwrap();
    let on = p.price(&quote(None), tuesday()).await.unwrap();
    assert!(on.lines.iter().any(|l| l.kind == LineKind::Corporate));

    assert!(p.set_corporate_active(TENANT, Uuid::new_v4(), false).await.is_err());
}
