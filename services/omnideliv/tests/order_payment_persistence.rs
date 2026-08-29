//! The prepaid-checkout columns (migration 0022), round-tripped through a
//! real Postgres.
//!
//! Every other test for `PaymentMethod`/`PaymentStatus` exercises pure
//! domain logic or a mocked repository — none of them prove the migration's
//! SQL is actually valid, or that `PgOrderRepository`'s column list and
//! `.bind()` order actually line up with it. This is exactly the shape of
//! gap this codebase has been bitten by before (see CLAUDE.md's
//! `project_ci_compliance_tests_never_ran.md` and friends): a change that
//! compiles and unit-tests green while the one thing that only a live
//! database can catch — a migration/repo mismatch — goes unexercised.
//!
//! Requires a running Postgres: skipped locally when DATABASE_URL is unset,
//! fatal in CI where a missing database means the harness broke.

use uuid::Uuid;

use logisticos_omnideliv::domain::entities::{
    Basket, Order, OrderStatus, PaymentMethod, PaymentStatus, Vendor, VendorLeg, Vertical,
};
use logisticos_omnideliv::domain::repositories::{BasketRepository, OrderRepository, VendorRepository};
use logisticos_omnideliv::infrastructure::db::{PgBasketRepository, PgOrderRepository, PgVendorRepository};

fn database_url() -> Option<String> {
    match std::env::var("DATABASE_URL") {
        Ok(url) => Some(url),
        Err(_) => {
            assert!(
                std::env::var("CI").is_err(),
                "DATABASE_URL is unset while CI is set — Postgres provisioning failed."
            );
            eprintln!("skipping: DATABASE_URL not set (this is fatal when CI is set)");
            None
        }
    }
}

async fn connect(url: &str) -> sqlx::PgPool {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .after_connect(|c, _| Box::pin(async move {
            sqlx::query("SET search_path TO omnideliv, public").execute(&mut *c).await?;
            Ok(())
        }))
        .connect(url).await.expect("connect");

    logisticos_common::migrations::run(&pool, "omnideliv", &sqlx::migrate!("./migrations"))
        .await.expect("migrate");
    pool
}

/// A real basket row — `orders.basket_id` is a foreign key, so a synthetic
/// UUID with nothing behind it fails at the database, not at the domain
/// layer this crate's other tests stop at.
async fn a_basket(pool: &sqlx::PgPool, tenant: Uuid) -> Uuid {
    let baskets = PgBasketRepository::new(pool.clone());
    let basket = Basket::new(tenant, Uuid::new_v4());
    baskets.save(&basket).await.expect("save basket");
    basket.id
}

/// A real, active vendor row — `order_vendor_legs.vendor_id` is also a
/// foreign key.
async fn a_vendor(pool: &sqlx::PgPool, tenant: Uuid) -> Uuid {
    let vendors = PgVendorRepository::new(pool.clone());
    let mut vendor = Vendor::new(
        tenant, Vertical::Restaurant, "Kuya's Lutong Bahay".into(),
        "123 Mabini St".into(), 14.5995, 120.9842,
    );
    vendor.activate();
    vendors.save(&vendor).await.expect("save vendor");
    vendor.id
}

/// A COD order — the default, untouched-by-`with_payment` shape — must save
/// and load back with every prepaid-checkout column at its default: this is
/// the byte-identical-to-today guarantee, proven against the real schema
/// rather than only against in-memory structs.
#[tokio::test]
async fn a_cod_order_round_trips_with_every_new_column_at_its_default() {
    let Some(url) = database_url() else { return };
    let pool = connect(&url).await;
    let tenant = Uuid::new_v4();
    let basket_id = a_basket(&pool, tenant).await;
    let vendor_id = a_vendor(&pool, tenant).await;
    let repo = PgOrderRepository::new(pool);

    let leg = VendorLeg::settle(tenant, vendor_id, 34_000, 1_500);
    let order = Order::place(
        tenant, Uuid::new_v4(), basket_id, Uuid::new_v4(),
        vec![leg], 7_900, 4_000, 5_800, 14.5995, 120.9842,
    );
    let order_id = order.id;

    repo.save(&order).await.expect("save cod order");
    let loaded = repo.find_by_id(tenant, order_id).await.expect("query").expect("exists");

    assert_eq!(loaded.payment_method, PaymentMethod::Cod);
    assert_eq!(loaded.payment_status, PaymentStatus::Pending);
    assert_eq!(loaded.payment_intent_id, None);
    assert_eq!(loaded.prepaid_amount_cents, 0);
    assert_eq!(loaded.payment_authorized_at, None);
    assert_eq!(loaded.pending_offer_card, None);
    assert_eq!(loaded.cod_amount_cents(), loaded.grand_total_cents);
}

/// The full `Online` lifecycle — authorized, then captured — round-tripped
/// through three separate saves, the way checkout, the payment-intent
/// consumer, and the courier-milestone consumer each independently persist
/// the same order across the real process boundary this test crosses in
/// miniature.
#[tokio::test]
async fn an_online_order_round_trips_through_authorization_and_capture() {
    let Some(url) = database_url() else { return };
    let pool = connect(&url).await;
    let tenant = Uuid::new_v4();
    let basket_id = a_basket(&pool, tenant).await;
    let vendor_id = a_vendor(&pool, tenant).await;
    let repo = PgOrderRepository::new(pool);

    let leg = VendorLeg::settle(tenant, vendor_id, 34_000, 1_500);
    let mut order = Order::place(
        tenant, Uuid::new_v4(), basket_id, Uuid::new_v4(),
        vec![leg], 7_900, 4_000, 5_800, 14.5995, 120.9842,
    );
    let prepaid = order.grand_total_cents;
    order = order
        .with_payment(PaymentMethod::Online, prepaid)
        .with_pending_offer_card(Some(serde_json::json!({"v": 1, "pickups": 1})));
    let order_id = order.id;

    // 1. Checkout's own save — `Placed`, `Pending`, holding the card.
    repo.save(&order).await.expect("save placed order");
    let loaded = repo.find_by_id(tenant, order_id).await.expect("query").expect("exists");
    assert_eq!(loaded.status, OrderStatus::Placed);
    assert_eq!(loaded.payment_method, PaymentMethod::Online);
    assert_eq!(loaded.payment_status, PaymentStatus::Pending);
    assert_eq!(loaded.prepaid_amount_cents, prepaid);
    assert_eq!(loaded.pending_offer_card, Some(serde_json::json!({"v": 1, "pickups": 1})));
    assert_eq!(loaded.cod_amount_cents(), 0, "fully prepaid — the courier collects nothing");

    // 2. The payment.intent.authorized consumer's save — offered, authorized.
    let intent_id = Uuid::new_v4();
    let mut order = loaded;
    order.payment_authorized(intent_id).unwrap();
    order.courier_task_id = Some(Uuid::new_v4());
    order.courier_offered().unwrap();
    repo.save(&order).await.expect("save authorized order");
    let loaded = repo.find_by_id(tenant, order_id).await.expect("query").expect("exists");
    assert_eq!(loaded.status, OrderStatus::AwaitingCourier);
    assert_eq!(loaded.payment_status, PaymentStatus::Authorized);
    assert_eq!(loaded.payment_intent_id, Some(intent_id));
    assert!(loaded.payment_authorized_at.is_some());
    // The card, and the prepaid split, must survive an update save untouched
    // — this is exactly the COALESCE-vs-plain-assignment distinction the
    // migration's ON CONFLICT clause has to get right.
    assert_eq!(loaded.pending_offer_card, Some(serde_json::json!({"v": 1, "pickups": 1})));
    assert_eq!(loaded.prepaid_amount_cents, prepaid);

    // 3. The courier-milestone consumer's save — a courier accepted, captured.
    let mut order = loaded;
    order.courier_claimed(order.courier_task_id.unwrap(), None).unwrap();
    order.payment_captured().unwrap();
    repo.save(&order).await.expect("save captured order");
    let loaded = repo.find_by_id(tenant, order_id).await.expect("query").expect("exists");
    assert_eq!(loaded.status, OrderStatus::Collecting);
    assert_eq!(loaded.payment_status, PaymentStatus::Captured);
    assert_eq!(loaded.payment_intent_id, Some(intent_id), "the intent id must survive too");
}

/// `find_awaiting_courier` — the recovery sweep's own query — must surface
/// an authorized-but-unclaimed online order with every payment field intact,
/// since `handle_online`'s void-timeout decision reads `payment_status` and
/// `payment_authorized_at` directly off what this query returns.
#[tokio::test]
async fn find_awaiting_courier_carries_the_payment_fields_the_recovery_sweep_needs() {
    let Some(url) = database_url() else { return };
    let pool = connect(&url).await;
    let tenant = Uuid::new_v4();
    let basket_id = a_basket(&pool, tenant).await;
    let repo = PgOrderRepository::new(pool);

    let mut order = Order::place(
        tenant, Uuid::new_v4(), basket_id, Uuid::new_v4(),
        vec![], 4_900, 0, 3_500, 14.5995, 120.9842,
    );
    let prepaid = order.grand_total_cents;
    order = order.with_payment(PaymentMethod::Online, prepaid);
    let intent_id = Uuid::new_v4();
    order.payment_authorized(intent_id).unwrap();
    order.status = OrderStatus::AwaitingCourier; // as the payment.intent.authorized consumer would leave it

    repo.save(&order).await.expect("save");

    let stuck = repo.find_awaiting_courier().await.expect("query");
    let found = stuck.iter().find(|o| o.id == order.id).expect("must be in the awaiting-courier set");

    assert_eq!(found.payment_method, PaymentMethod::Online);
    assert_eq!(found.payment_status, PaymentStatus::Authorized);
    assert_eq!(found.payment_intent_id, Some(intent_id));
    assert!(found.payment_authorized_at.is_some());
}
