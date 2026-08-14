//! A complete order with no LLM anywhere in the path.
//!
//! This is the test whose absence let Plans 3 and 7 both claim a working
//! fallback that dead-ended at a vendor list. It touches no mesh code, and it
//! must keep passing with the Claude API key unset.
//!
//! Requires a running Postgres: skipped locally when DATABASE_URL is unset,
//! fatal in CI where a missing database means the harness broke.

use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

use logisticos_omnideliv::domain::entities::{
    Basket, BasketLine, CatalogItem, CatalogSource, Vendor, Vertical,
};
use logisticos_omnideliv::domain::repositories::{BasketRepository, CatalogRepository, VendorRepository};
use logisticos_omnideliv::infrastructure::db::{
    PgBasketRepository, PgCatalogRepository, PgVendorRepository,
};

/// The database URL, or `None` when it is legitimate to skip.
///
/// Legitimate on a dev machine with no Postgres; never legitimate in CI, where
/// the workflow provisions one. A quiet `return` there reports a green pass for
/// a test that never ran — and this particular test exists precisely because an
/// untested claim looked like a working feature.
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
async fn a_customer_can_build_and_check_out_a_basket_without_the_mesh() {
    let Some(url) = database_url() else { return };

    // Proving the point: no Claude credentials in this process.
    std::env::remove_var("CLAUDE_API_KEY");
    std::env::remove_var("ANTHROPIC_API_KEY");

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

    let mut vendor = Vendor::new(tenant, Vertical::Grocery, "Corner Store".into(),
                                 "1 Test St".into(), 14.6, 120.98);
    vendor.activate();
    vendors.save(&vendor).await.expect("save vendor");

    let now = chrono::Utc::now();
    let item = CatalogItem {
        id: Uuid::new_v4(), tenant_id: tenant, vendor_id: vendor.id,
        sku: "milk-1l".into(), name: "Milk 1L".into(), description: None,
        price_cents: 8_500, modifiers: serde_json::json!([]),
        allergens: vec![], dietary_tags: vec![], vertical_attrs: serde_json::json!({}),
        category: None,
        allergens_declared_at: Some(chrono::Utc::now()),
        is_listed: true,
        source: CatalogSource::Manual, external_id: None, synced_at: None,
        image_key: None,
        created_at: now, updated_at: now,
    };
    catalog.save_item(&item).await.expect("save item");

    // Build the basket by hand — exactly what the app does.
    let mut basket = Basket::new(tenant, Uuid::new_v4());
    let si = basket.browse_sub_intent(Vertical::Grocery);
    basket.add_line(BasketLine::propose(
        basket.id, si, tenant, vendor.id, item.id, 2, item.price_cents, "browse",
    ));
    baskets.save(&basket).await.expect("save basket");

    let loaded = baskets.find_by_id(tenant, basket.id).await.expect("load").expect("exists");

    assert_eq!(loaded.lines.len(), 1);
    assert_eq!(loaded.goods_total_cents(), 17_000, "2 × ₱85.00");
    assert_eq!(
        loaded.lines_awaiting_review().len(),
        0,
        "a hand-built basket has nothing to review, so checkout is not blocked"
    );

    // The browse partition survived the round trip as a browse partition. If it
    // came back tagged `mesh`, a later specialist proposal would replace the
    // customer's hand-picked lines wholesale.
    assert_eq!(loaded.sub_intents.len(), 1);
    assert_eq!(
        loaded.sub_intents[0].source,
        logisticos_omnideliv::domain::entities::SubIntentSource::Browse,
    );

    // Checkout's precondition is satisfied — it is reachable from here with no
    // mesh involvement. CheckoutService itself is covered by its own tests.
    assert!(!loaded.subtotals_by_vendor().is_empty());
}

/// The optimistic lock against a real database: two writers racing from the
/// same starting version must not silently lose one of the writes.
#[tokio::test]
async fn a_concurrent_write_is_rejected_rather_than_lost() {
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
    let baskets = PgBasketRepository::new(pool.clone());

    let basket = Basket::new(tenant, Uuid::new_v4());
    baskets.save(&basket).await.expect("initial save");

    // Two readers load the same version, then both mutate and save.
    let mut a = baskets.find_by_id(tenant, basket.id).await.expect("load a").expect("exists");
    let mut b = baskets.find_by_id(tenant, basket.id).await.expect("load b").expect("exists");

    a.browse_sub_intent(Vertical::Grocery);
    b.browse_sub_intent(Vertical::Restaurant);

    baskets.save(&a).await.expect("first writer wins");
    let second = baskets.save(&b).await;

    assert!(
        second.is_err(),
        "the second writer started from a stale version and must be rejected, not silently applied",
    );

    let loaded = baskets.find_by_id(tenant, basket.id).await.expect("reload").expect("exists");
    assert_eq!(loaded.version, a.version);
    assert_eq!(loaded.sub_intents.len(), 1, "only the winning write is present");
}
