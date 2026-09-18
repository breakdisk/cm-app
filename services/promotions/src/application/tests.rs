use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::*;
use crate::domain::offer::AccountHistory;

/// The ledger's rules, as the migration's unique indexes enforce them.
#[derive(Default)]
struct Fake {
    offers: Mutex<Vec<Offer>>,
    redemptions: Mutex<Vec<(NewRedemption, bool)>>, // (row, released)
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
        for (r, released) in self.redemptions.lock().unwrap().iter() {
            if r.tenant_id == tenant_id && r.account_id == account_id && !released {
                if r.offer.windowed && r.month == month {
                    h.windowed_used_this_month = true;
                }
                h.offers_used.push(r.offer.id);
            }
        }
        Ok(h)
    }
    async fn redeem(&self, r: &NewRedemption) -> anyhow::Result<RedeemOutcome> {
        let mut all = self.redemptions.lock().unwrap();
        if all.iter().any(|(x, _)| x.shipment_id == r.shipment_id) {
            return Ok(RedeemOutcome::AlreadyForThisBooking);
        }
        let standing = |x: &NewRedemption, released: bool| !released && x.tenant_id == r.tenant_id && x.account_id == r.account_id;
        if r.offer.windowed && all.iter().any(|(x, rel)| standing(x, *rel) && x.offer.windowed && x.month == r.month) {
            return Ok(RedeemOutcome::MonthTaken);
        }
        if r.offer.once_per_account && all.iter().any(|(x, rel)| standing(x, *rel) && x.offer.id == r.offer.id) {
            return Ok(RedeemOutcome::OfferTaken);
        }
        all.push((r.clone(), false));
        Ok(RedeemOutcome::Redeemed)
    }
    async fn release(&self, shipment_id: Uuid, _at: DateTime<Utc>) -> anyhow::Result<bool> {
        let mut all = self.redemptions.lock().unwrap();
        match all.iter_mut().find(|(x, rel)| x.shipment_id == shipment_id && !*rel) {
            Some(row) => {
                row.1 = true;
                Ok(true)
            }
            None => Ok(false),
        }
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

const TENANT: Uuid = Uuid::from_u128(1);
const ME: Uuid = Uuid::from_u128(2);

fn rules() -> Rules {
    Rules { window: WindowRule { from_day: 11, to_day: 24 }, ceiling_pct: 20, ceiling_flat_units: 50, utc_offset_minutes: 480 }
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

fn service(offers: Vec<Offer>) -> Promotions {
    let fake = Fake { offers: Mutex::new(offers), ..Default::default() };
    Promotions::new(Arc::new(fake), rules())
}

fn quote(code: Option<&str>) -> PriceRequest {
    PriceRequest {
        tenant_id: TENANT,
        account_id: ME,
        code: code.map(str::to_owned),
        currency: "PHP".into(),
        carriage_cents: 30_000,
        accessorial_cents: 5_000,
    }
}

fn redemption(shipment: u128) -> RedeemRequest {
    RedeemRequest {
        tenant_id: TENANT,
        account_id: ME,
        shipment_id: Uuid::from_u128(shipment),
        code: "move20".into(),
        discount_cents: 2_500,
        currency: "PHP".into(),
    }
}

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
    p.redeem(&redemption(100), tuesday()).await.unwrap();
    let second = p.redeem(&redemption(101), tuesday()).await;
    assert!(matches!(second, Err(AppError::Conflict(ref c)) if c == "ALREADY_REDEEMED_THIS_MONTH"));
}

#[tokio::test]
async fn a_retried_booking_is_not_a_second_spend() {
    let p = service(vec![move20()]);
    p.redeem(&redemption(100), tuesday()).await.unwrap();
    p.redeem(&redemption(100), tuesday()).await.unwrap();
}

/// Cancelled: the code goes back, and the month is open again.
#[tokio::test]
async fn a_cancelled_booking_gives_the_code_back() {
    let p = service(vec![move20()]);
    p.redeem(&redemption(100), tuesday()).await.unwrap();
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
    p.redeem(&RedeemRequest { code: "WELCOME15".into(), discount_cents: 1_500, ..redemption(100) }, tuesday())
        .await
        .unwrap();

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
