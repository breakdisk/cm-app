//! The two clocks, against a real database.
//!
//! The confirmation model is only as good as the SQL that stores it, and the
//! interesting parts of that SQL cannot be reached by a trait fake: the
//! `CASE WHEN $5 THEN NOW() ELSE item_availability.confirmed_at END` upsert, the
//! partial unique index the ingest port matches on, and whether migration 0015's
//! backfill leaves an existing catalog trusted rather than flipping the whole
//! thing to "never confirmed".
//!
//! Runs the service's own migration path — `logisticos_common::migrations::run`
//! with the same schema name `bootstrap.rs` passes — rather than `sqlx migrate
//! run`, because that helper pre-creates the schema and the service-owned
//! `_sqlx_migrations` table, and a test that migrates differently from the
//! service proves the wrong thing.
//!
//! Requires a running Postgres: skipped locally when DATABASE_URL is unset,
//! fatal in CI where a missing database means the harness broke rather than
//! that there is nothing to test.

use chrono::Utc;
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use logisticos_omnideliv::domain::entities::{
    Availability, AvailabilityState, CatalogItem, CatalogSource, Confidence, IngestedItem, Vendor,
    Vertical,
};
use logisticos_omnideliv::domain::repositories::{CatalogRepository, VendorRepository};
use logisticos_omnideliv::infrastructure::db::{PgCatalogRepository, PgVendorRepository};

const FRESH_WINDOW: i64 = 30;

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

async fn pool(url: &str) -> PgPool {
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .after_connect(|c, _| {
            Box::pin(async move {
                sqlx::query("SET search_path TO omnideliv, public")
                    .execute(&mut *c)
                    .await?;
                Ok(())
            })
        })
        .connect(url)
        .await
        .expect("connect");

    // Exactly what bootstrap.rs does, including the schema name.
    logisticos_common::migrations::run(&pool, "omnideliv", &sqlx::migrate!("./migrations"))
        .await
        .expect("migrations must apply cleanly");

    pool
}

/// A fresh vendor per test, so runs do not collide on the SKU unique index.
async fn a_vendor(vendors: &PgVendorRepository, tenant: Uuid) -> Vendor {
    let v = Vendor::new(
        tenant,
        Vertical::Restaurant,
        "Kuya's".into(),
        "12 Mabini St".into(),
        14.5995,
        120.9842,
    );
    vendors.save(&v).await.expect("save vendor");
    v
}

fn an_item(tenant: Uuid, vendor_id: Uuid, sku: &str) -> CatalogItem {
    let now = Utc::now();
    CatalogItem {
        id: Uuid::new_v4(),
        tenant_id: tenant,
        vendor_id,
        sku: sku.into(),
        name: "Chicken Adobo".into(),
        description: None,
        price_cents: 18000,
        modifiers: Vec::new(),
        allergens: vec![],
        allergens_declared_at: None,
        dietary_tags: vec![],
        category: None,
        vertical_attrs: serde_json::json!({}),
        is_listed: true,
        source: CatalogSource::Manual,
        external_id: None,
        synced_at: None,
        image_key: None,
        created_at: now,
        updated_at: now,
    }
}

/// Migration 0015 applies, and the columns it adds round-trip through the
/// mapper. If `source` did not survive, `map_pair` would refuse the row outright
/// rather than silently relabel it — so a clean read is the assertion.
#[tokio::test]
async fn the_migration_applies_and_provenance_round_trips() {
    let Some(url) = database_url() else { return };
    let pool = pool(&url).await;
    let tenant = Uuid::new_v4();
    let vendors = PgVendorRepository::new(pool.clone());
    let catalog = PgCatalogRepository::new(pool.clone());

    let vendor = a_vendor(&vendors, tenant).await;
    let mut item = an_item(tenant, vendor.id, "ADOBO-1");
    item.source = CatalogSource::Shopify;
    item.external_id = Some("gid://shopify/Product/1".into());
    item.synced_at = Some(Utc::now());
    catalog.save_item(&item).await.expect("save");

    let read = catalog
        .find_item(tenant, item.id)
        .await
        .expect("read")
        .expect("the item exists");

    assert_eq!(read.source, CatalogSource::Shopify);
    assert_eq!(read.external_id.as_deref(), Some("gid://shopify/Product/1"));
    assert!(read.synced_at.is_some());
}

/// The load-bearing SQL. A machine write must move `updated_at` and leave
/// `confirmed_at` exactly where the human left it — the `CASE WHEN` in the
/// upsert, which no fake can exercise.
#[tokio::test]
async fn a_machine_write_leaves_the_human_clock_alone() {
    let Some(url) = database_url() else { return };
    let pool = pool(&url).await;
    let tenant = Uuid::new_v4();
    let vendors = PgVendorRepository::new(pool.clone());
    let catalog = PgCatalogRepository::new(pool.clone());

    let vendor = a_vendor(&vendors, tenant).await;
    let item = an_item(tenant, vendor.id, "ADOBO-2");
    catalog.save_item(&item).await.expect("save");

    // A person confirms it.
    let user = Uuid::new_v4();
    catalog
        .set_availability(&Availability {
            item_id: item.id,
            tenant_id: tenant,
            state: AvailabilityState::Available,
            updated_at: Utc::now(),
            confirmed_at: Some(Utc::now()),
            updated_by: Some(user),
        })
        .await
        .expect("human confirm");

    let confirmed_at: Option<chrono::DateTime<Utc>> =
        sqlx::query("SELECT confirmed_at FROM omnideliv.item_availability WHERE item_id = $1")
            .bind(item.id)
            .fetch_one(&pool)
            .await
            .expect("read")
            .get("confirmed_at");
    let confirmed_at = confirmed_at.expect("a human confirmation must be stamped");

    // Then a sync writes the same row with no human behind it.
    catalog
        .set_availability(&Availability {
            item_id: item.id,
            tenant_id: tenant,
            state: AvailabilityState::Available,
            updated_at: Utc::now(),
            confirmed_at: None,
            updated_by: None,
        })
        .await
        .expect("machine write");

    let row = sqlx::query(
        "SELECT confirmed_at, updated_at, updated_by
           FROM omnideliv.item_availability WHERE item_id = $1",
    )
    .bind(item.id)
    .fetch_one(&pool)
    .await
    .expect("read");

    let after: Option<chrono::DateTime<Utc>> = row.get("confirmed_at");
    let updated: chrono::DateTime<Utc> = row.get("updated_at");
    let updated_by: Option<Uuid> = row.get("updated_by");

    assert_eq!(
        after,
        Some(confirmed_at),
        "a sync moved the human confirmation clock",
    );
    assert!(updated >= confirmed_at, "the machine clock must still advance");
    assert_eq!(
        updated_by,
        Some(user),
        "a machine write must not erase who last attested",
    );
}

/// Nobody has ever confirmed an imported item, and the domain agrees when the
/// row comes back out of Postgres rather than out of a struct literal.
#[tokio::test]
async fn an_imported_item_reads_back_as_never_confirmed() {
    let Some(url) = database_url() else { return };
    let pool = pool(&url).await;
    let tenant = Uuid::new_v4();
    let vendors = PgVendorRepository::new(pool.clone());
    let catalog = PgCatalogRepository::new(pool.clone());

    let vendor = a_vendor(&vendors, tenant).await;
    let item = an_item(tenant, vendor.id, "ADOBO-3");
    catalog.save_item(&item).await.expect("save");

    let listed = catalog.list_for_vendor(tenant, vendor.id).await.expect("list");
    let found = listed
        .iter()
        .find(|i| i.item.id == item.id)
        .expect("the item is listed");

    assert!(found.availability.confirmed_at.is_none());
    assert_eq!(found.availability.confidence(FRESH_WINDOW), Confidence::Uncertain);
    assert!(found.availability.warrants_substitute(FRESH_WINDOW));
}

/// The ingest port's match keys, against the real indexes.
#[tokio::test]
async fn ingest_matches_on_external_id_across_a_sku_rename() {
    let Some(url) = database_url() else { return };
    let pool = pool(&url).await;
    let tenant = Uuid::new_v4();
    let vendors = PgVendorRepository::new(pool.clone());
    let catalog = PgCatalogRepository::new(pool.clone());

    let vendor = a_vendor(&vendors, tenant).await;
    let mut item = an_item(tenant, vendor.id, "OLD-SKU");
    item.source = CatalogSource::Shopify;
    item.external_id = Some("gid://shopify/Product/7".into());
    catalog.save_item(&item).await.expect("save");

    let by_sku = catalog
        .find_item_by_sku(tenant, vendor.id, "OLD-SKU")
        .await
        .expect("by sku");
    assert_eq!(by_sku.map(|i| i.id), Some(item.id));

    let by_ext = catalog
        .find_item_by_external(tenant, vendor.id, CatalogSource::Shopify, "gid://shopify/Product/7")
        .await
        .expect("by external");
    assert_eq!(by_ext.map(|i| i.id), Some(item.id));

    // Another tenant must not resolve either key, whatever it knows.
    let stranger = Uuid::new_v4();
    assert!(catalog
        .find_item_by_sku(stranger, vendor.id, "OLD-SKU")
        .await
        .expect("by sku")
        .is_none());
    assert!(catalog
        .find_item_by_external(stranger, vendor.id, CatalogSource::Shopify, "gid://shopify/Product/7")
        .await
        .expect("by external")
        .is_none());
}

/// Bulk confirm reaches exactly the rows it should: this vendor's listed,
/// in-stock items — and not a neighbouring store's, and not the ones the vendor
/// has marked gone.
#[tokio::test]
async fn confirm_all_spares_out_of_stock_and_other_stores() {
    let Some(url) = database_url() else { return };
    let pool = pool(&url).await;
    let tenant = Uuid::new_v4();
    let vendors = PgVendorRepository::new(pool.clone());
    let catalog = PgCatalogRepository::new(pool.clone());

    let mine = a_vendor(&vendors, tenant).await;
    let theirs = a_vendor(&vendors, tenant).await;

    let in_stock = an_item(tenant, mine.id, "IN-1");
    let gone     = an_item(tenant, mine.id, "GONE-1");
    let neighbour = an_item(tenant, theirs.id, "THEIRS-1");
    for i in [&in_stock, &gone, &neighbour] {
        catalog.save_item(i).await.expect("save");
    }

    catalog
        .set_availability(&Availability {
            item_id: gone.id,
            tenant_id: tenant,
            state: AvailabilityState::OutOfStock,
            updated_at: Utc::now(),
            confirmed_at: None,
            updated_by: None,
        })
        .await
        .expect("mark gone");

    let user = Uuid::new_v4();
    let n = catalog
        .confirm_all_for_vendor(tenant, mine.id, user)
        .await
        .expect("confirm all");

    assert_eq!(n, 1, "only the listed, in-stock item of this store");

    let confirmed = |id: Uuid| {
        let pool = pool.clone();
        async move {
            let c: Option<chrono::DateTime<Utc>> = sqlx::query(
                "SELECT confirmed_at FROM omnideliv.item_availability WHERE item_id = $1",
            )
            .bind(id)
            .fetch_one(&pool)
            .await
            .expect("read")
            .get("confirmed_at");
            c.is_some()
        }
    };

    assert!(confirmed(in_stock.id).await, "the in-stock item is confirmed");
    assert!(
        !confirmed(gone.id).await,
        "confirming a store must not quietly un-mark what the vendor said was gone",
    );
    assert!(
        !confirmed(neighbour.id).await,
        "one store's confirmation must never reach another's shelves",
    );
}

/// The whole ingest path end to end, through real SQL: create, then re-sync the
/// same batch and see updates rather than duplicates — and no confirmation.
#[tokio::test]
async fn a_repeated_ingest_updates_rather_than_duplicating() {
    let Some(url) = database_url() else { return };
    let pool = pool(&url).await;
    let tenant = Uuid::new_v4();
    let vendors = PgVendorRepository::new(pool.clone());
    let catalog = PgCatalogRepository::new(pool.clone());
    let vendor = a_vendor(&vendors, tenant).await;

    let svc = logisticos_omnideliv::application::services::CatalogService::new(
        std::sync::Arc::new(PgVendorRepository::new(pool.clone())),
        std::sync::Arc::new(PgCatalogRepository::new(pool.clone())),
        FRESH_WINDOW,
    );

    let batch = || {
        vec![IngestedItem {
            external_id: Some("gid://shopify/Product/11".into()),
            sku: "SYNC-1".into(),
            name: "Pancit".into(),
            description: None,
            price_cents: 15000,
            allergens: vec!["shellfish".into()],
            dietary_tags: vec![],
            is_listed: true,
        }]
    };

    let first = svc
        .ingest_for_vendor(tenant, vendor.id, CatalogSource::Shopify, batch())
        .await
        .expect("first ingest");
    assert_eq!((first.created, first.updated), (1, 0));

    let second = svc
        .ingest_for_vendor(tenant, vendor.id, CatalogSource::Shopify, batch())
        .await
        .expect("second ingest");
    assert_eq!(
        (second.created, second.updated),
        (0, 1),
        "a re-run must match its own rows, not duplicate them",
    );

    let items = catalog.list_for_vendor(tenant, vendor.id).await.expect("list");
    assert_eq!(items.len(), 1, "one row, not two");

    let only = &items[0];
    assert_eq!(only.item.allergens, vec!["shellfish".to_string()],
               "machine allergen data is stored — we can still exclude on it");
    assert!(
        only.item.allergens_declared_at.is_none(),
        "but a sync never declares contents",
    );
    assert!(
        only.availability.confirmed_at.is_none(),
        "and never confirms stock",
    );
}

/// A vendor id belonging to no vendor of this tenant is refused before any
/// write — against the real foreign key, which would otherwise accept it.
#[tokio::test]
async fn a_cross_tenant_ingest_is_refused_by_the_service_not_the_foreign_key() {
    let Some(url) = database_url() else { return };
    let pool = pool(&url).await;
    let tenant = Uuid::new_v4();
    let other  = Uuid::new_v4();
    let vendors = PgVendorRepository::new(pool.clone());
    let vendor = a_vendor(&vendors, tenant).await;

    let svc = logisticos_omnideliv::application::services::CatalogService::new(
        std::sync::Arc::new(PgVendorRepository::new(pool.clone())),
        std::sync::Arc::new(PgCatalogRepository::new(pool.clone())),
        FRESH_WINDOW,
    );

    let err = svc
        .ingest_for_vendor(other, vendor.id, CatalogSource::Pos, vec![IngestedItem {
            external_id: None,
            sku: "SMUGGLED-1".into(),
            name: "Smuggled".into(),
            description: None,
            price_cents: 100,
            allergens: vec![],
            dietary_tags: vec![],
            is_listed: true,
        }])
        .await;

    assert!(err.is_err(), "the service must refuse it");

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM omnideliv.catalog_items WHERE sku = 'SMUGGLED-1'",
    )
    .fetch_one(&pool)
    .await
    .expect("count");
    assert_eq!(count, 0, "and nothing may reach the table");
}
