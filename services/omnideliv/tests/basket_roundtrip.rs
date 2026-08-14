//! The basket aggregate must survive a save/load round trip with its
//! substitution chain intact. Requires a running Postgres: skipped locally
//! when DATABASE_URL is unset, fatal in CI where a missing database means the
//! harness broke rather than that there is nothing to test.

use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

use logisticos_omnideliv::domain::entities::{
    Basket, BasketDelta, BasketLine, CatalogItem, CatalogSource, LineState, SubIntent, SubIntentSource,
    SubIntentStatus,
    Vendor, Vertical,
};
use logisticos_omnideliv::domain::repositories::{BasketRepository, CatalogRepository, VendorRepository};
use logisticos_omnideliv::infrastructure::db::{
    PgBasketRepository, PgCatalogRepository, PgVendorRepository,
};

/// The database URL, or `None` when it is legitimate to skip.
///
/// Legitimate on a dev machine with no Postgres; never legitimate in CI, where
/// the workflow provisions Postgres and runs this service's migrations before
/// the test step. A quiet `return` there reports a green pass for a test that
/// never ran, which is indistinguishable from a real one.
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

#[tokio::test]
async fn a_basket_with_a_substitution_chain_survives_a_round_trip() {
    let Some(url) = database_url() else { return };

    let pool = PgPoolOptions::new()
        .after_connect(|c, _| Box::pin(async move {
            sqlx::query("SET search_path TO omnideliv, public").execute(&mut *c).await?;
            Ok(())
        }))
        .connect(&url).await.expect("connect");

    logisticos_common::migrations::run(&pool, "omnideliv", &sqlx::migrate!("./migrations"))
        .await.expect("migrate");

    let tenant = Uuid::new_v4();
    let vendors = PgVendorRepository::new(pool.clone());
    let catalog = PgCatalogRepository::new(pool.clone());
    let baskets = PgBasketRepository::new(pool.clone());

    // Seed a vendor and two items — the original and its replacement.
    let mut vendor = Vendor::new(tenant, Vertical::Grocery, "Test Grocery".into(),
                                 "1 Test St".into(), 14.6, 120.98);
    vendor.activate();
    vendors.save(&vendor).await.expect("save vendor");

    let mk_item = |sku: &str, price: i64| {
        let now = chrono::Utc::now();
        CatalogItem {
            id: Uuid::new_v4(), tenant_id: tenant, vendor_id: vendor.id,
            sku: sku.into(), name: sku.into(), description: None, price_cents: price,
            modifiers: serde_json::json!([]), allergens: vec![], dietary_tags: vec![],
            allergens_declared_at: Some(now),
            vertical_attrs: serde_json::json!({}), is_listed: true,
            source: CatalogSource::Manual, external_id: None, synced_at: None,
            image_key: None,
            created_at: now, updated_at: now,
        }
    };
    let original = mk_item("eggs-brand-a", 12_000);
    let replacement = mk_item("eggs-brand-b", 10_800);
    catalog.save_item(&original).await.expect("save original");
    catalog.save_item(&replacement).await.expect("save replacement");

    // Build a basket where the replacement points at the original.
    let mut basket = Basket::new(tenant, Uuid::new_v4());
    let si = SubIntent {
        id: Uuid::new_v4(), basket_id: basket.id, tenant_id: tenant,
        vertical: Vertical::Grocery, vendor_hint: None,
        raw_text: "we're out of milk and eggs".into(),
        constraints: serde_json::json!({}), status: SubIntentStatus::Pending,
        source: SubIntentSource::Mesh,
        created_at: chrono::Utc::now(),
    };
    basket.sub_intents.push(si.clone());

    let mut out_of_stock = BasketLine::propose(
        basket.id, si.id, tenant, vendor.id, original.id, 1, 12_000, "nutritionist");
    out_of_stock.state = LineState::Rejected;

    let mut swap = BasketLine::propose(
        basket.id, si.id, tenant, vendor.id, replacement.id, 1, 10_800, "nutritionist");
    swap.state = LineState::Substituted;
    swap.substitution_for = Some(out_of_stock.id);

    // Order matters: the replacement's FK points at the original, so the
    // original must be inserted first. `apply` preserves this order.
    basket.apply(BasketDelta { sub_intent_id: si.id, lines: vec![out_of_stock, swap], note: None });

    baskets.save(&basket).await.expect("save basket");

    let loaded = baskets.find_by_id(tenant, basket.id).await
        .expect("load")
        .expect("basket should exist");

    assert_eq!(loaded.lines.len(), 2);
    assert_eq!(loaded.goods_total_cents(), 10_800,
               "the rejected original must not be charged for");
    assert_eq!(loaded.lines_awaiting_review().len(), 1,
               "the substitution is the one decision blocking checkout");

    let chained = loaded.lines.iter().find(|l| l.substitution_for.is_some())
        .expect("substitution chain must survive the round trip");
    assert_eq!(chained.state, LineState::Substituted);
}
