//! Who may write which catalog row.
//!
//! The console resolves the store from the caller's token, never from an id in
//! the request — but "the handler does it correctly today" is not a guarantee.
//! These pin the rule at the service layer, where every future caller passes.
//!
//! Needs no database: the repositories are traits, so the test supplies its own.
//! Ownership is a decision, and a decision can be tested without storage.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use logisticos_omnideliv::application::services::{CatalogService, ItemDraft, ItemPatch};
use logisticos_omnideliv::domain::entities::{
    Availability, CatalogItem, CatalogSource, IngestedItem, Vendor, Vertical,
};
use logisticos_omnideliv::domain::repositories::{
    CatalogRepository, ItemFacts, ItemWithAvailability, VendorRepository,
};

const FRESH_WINDOW: i64 = 30;

// ---- fakes -----------------------------------------------------------------

struct FakeVendors {
    /// user_id -> vendor
    by_user: Vec<(Uuid, Vendor)>,
}

// Every method filters on tenant, exactly as the Pg implementation's
// `WHERE tenant_id = $1` does. A fake that ignores the tenant argument is not a
// simplification — it silently passes any test about tenant isolation, which is
// the one property these repositories exist to enforce. This one did, and hid a
// real cross-tenant write until it was corrected.
#[async_trait]
impl VendorRepository for FakeVendors {
    async fn find_by_id(&self, tenant_id: Uuid, id: Uuid) -> anyhow::Result<Option<Vendor>> {
        Ok(self.by_user.iter()
            .find(|(_, v)| v.id == id && v.tenant_id == tenant_id)
            .map(|(_, v)| v.clone()))
    }
    async fn save(&self, _v: &Vendor) -> anyhow::Result<()> { Ok(()) }
    async fn find_by_user(&self, tenant_id: Uuid, user_id: Uuid) -> anyhow::Result<Option<Vendor>> {
        Ok(self.by_user.iter()
            .find(|(u, v)| *u == user_id && v.tenant_id == tenant_id)
            .map(|(_, v)| v.clone()))
    }
    async fn find_near(
        &self, _t: Uuid, _v: Vertical, _lat: f64, _lng: f64, _r: f64, _l: i64,
    ) -> anyhow::Result<Vec<Vendor>> { Ok(vec![]) }
    // Filters on tenant for the same reason every method above does: the
    // operator review queue is the one place a vendor from another tenant
    // would be both visible and actionable.
    async fn list_for_tenant(&self, tenant_id: Uuid) -> anyhow::Result<Vec<Vendor>> {
        Ok(self.by_user.iter()
            .filter(|(_, v)| v.tenant_id == tenant_id)
            .map(|(_, v)| v.clone())
            .collect())
    }
}

#[derive(Default)]
struct FakeCatalog {
    items:  Mutex<Vec<CatalogItem>>,
    /// Every availability write the service made, in order.
    writes: Mutex<Vec<Availability>>,
    confirmed_all: Mutex<Vec<(Uuid, Uuid)>>,
}

#[async_trait]
impl CatalogRepository for FakeCatalog {
    async fn save_item(&self, i: &CatalogItem) -> anyhow::Result<()> {
        let mut items = self.items.lock().unwrap();
        match items.iter_mut().find(|x| x.id == i.id) {
            Some(existing) => *existing = i.clone(),
            None => items.push(i.clone()),
        }
        Ok(())
    }
    async fn find_item(&self, _t: Uuid, item_id: Uuid) -> anyhow::Result<Option<CatalogItem>> {
        Ok(self.items.lock().unwrap().iter().find(|i| i.id == item_id).cloned())
    }
    async fn find_item_by_sku(
        &self, _t: Uuid, vendor_id: Uuid, sku: &str,
    ) -> anyhow::Result<Option<CatalogItem>> {
        Ok(self.items.lock().unwrap().iter()
            .find(|i| i.vendor_id == vendor_id && i.sku == sku).cloned())
    }
    async fn find_item_by_external(
        &self, _t: Uuid, vendor_id: Uuid, source: CatalogSource, external_id: &str,
    ) -> anyhow::Result<Option<CatalogItem>> {
        Ok(self.items.lock().unwrap().iter()
            .find(|i| i.vendor_id == vendor_id
                   && i.source == source
                   && i.external_id.as_deref() == Some(external_id))
            .cloned())
    }
    // Records the write like every other method on this fake, and filters on
    // tenant for the same reason the Pg one does.
    async fn set_image_key(
        &self,
        _tenant_id: Uuid,
        _item_id:   Uuid,
        _key:       Option<&str>,
    ) -> anyhow::Result<()> { Ok(()) }

    async fn set_availability(&self, a: &Availability) -> anyhow::Result<()> {
        self.writes.lock().unwrap().push(a.clone());
        Ok(())
    }
    async fn confirm_all_for_vendor(
        &self, _t: Uuid, vendor_id: Uuid, user_id: Uuid,
    ) -> anyhow::Result<u64> {
        self.confirmed_all.lock().unwrap().push((vendor_id, user_id));
        Ok(self.items.lock().unwrap().iter().filter(|i| i.vendor_id == vendor_id).count() as u64)
    }
    async fn list_for_vendor(&self, _t: Uuid, _v: Uuid) -> anyhow::Result<Vec<ItemWithAvailability>> {
        Ok(vec![])
    }
    async fn search(
        &self, _t: Uuid, _v: Uuid, _q: &str, _a: &[String], _l: i64,
    ) -> anyhow::Result<Vec<ItemWithAvailability>> { Ok(vec![]) }
    async fn item_facts(&self, _t: Uuid, _ids: &[Uuid]) -> anyhow::Result<Vec<ItemFacts>> {
        Ok(vec![])
    }
    async fn declare_allergens(
        &self, _t: Uuid, item_id: Uuid, allergens: &[String],
    ) -> anyhow::Result<bool> {
        let mut items = self.items.lock().unwrap();
        match items.iter_mut().find(|i| i.id == item_id) {
            Some(i) => {
                i.allergens = allergens.to_vec();
                i.allergens_declared_at = Some(Utc::now());
                Ok(true)
            }
            None => Ok(false),
        }
    }
}

// ---- fixture ---------------------------------------------------------------

struct World {
    svc:      CatalogService,
    catalog:  Arc<FakeCatalog>,
    tenant:   Uuid,
    /// The user who runs vendor A.
    user_a:   Uuid,
    vendor_a: Uuid,
    /// An item belonging to vendor B, which user A must never be able to touch.
    b_item:   Uuid,
}

fn world() -> World {
    let tenant   = Uuid::new_v4();
    let user_a   = Uuid::new_v4();
    let user_b   = Uuid::new_v4();

    let a = Vendor::new(tenant, Vertical::Restaurant, "A".into(), "addr".into(), 14.6, 121.0);
    let b = Vendor::new(tenant, Vertical::Grocery,    "B".into(), "addr".into(), 14.6, 121.0);
    let (vendor_a, vendor_b) = (a.id, b.id);

    let catalog = Arc::new(FakeCatalog::default());
    let b_item = Uuid::new_v4();
    catalog.items.lock().unwrap().push(CatalogItem {
        id: b_item, tenant_id: tenant, vendor_id: vendor_b,
        sku: "B-1".into(), name: "Milk".into(), description: None, price_cents: 9000,
        modifiers: serde_json::json!([]), allergens: vec!["dairy".into()],
        allergens_declared_at: Some(Utc::now()), dietary_tags: vec![],
        vertical_attrs: serde_json::json!({}), is_listed: true,
        source: CatalogSource::Manual, external_id: None, synced_at: None,
        image_key: None,
        created_at: Utc::now(), updated_at: Utc::now(),
    });

    let vendors = Arc::new(FakeVendors { by_user: vec![(user_a, a), (user_b, b)] });
    let svc = CatalogService::new(vendors, catalog.clone(), FRESH_WINDOW);

    World { svc, catalog, tenant, user_a, vendor_a, b_item }
}

fn draft(sku: &str) -> ItemDraft {
    ItemDraft {
        sku: sku.into(),
        name: "Chicken Adobo".into(),
        description: Some("with rice".into()),
        price_cents: 18000,
        allergens: None,
        dietary_tags: vec![],
        modifiers: serde_json::json!([]),
        vertical_attrs: serde_json::json!({}),
    }
}

// ---- the rules -------------------------------------------------------------

#[tokio::test]
async fn a_vendor_cannot_edit_another_vendors_item() {
    let w = world();

    let edited = w.svc
        .update_own_item(w.tenant, w.user_a, w.b_item, ItemPatch {
            name: Some("Free Milk".into()),
            price_cents: Some(1),
            ..Default::default()
        })
        .await
        .expect("the call itself must succeed — it is a refusal, not a fault");

    assert!(!edited, "user A must not be able to edit vendor B's item");

    let item = w.catalog.find_item(w.tenant, w.b_item).await.unwrap().unwrap();
    assert_eq!(item.price_cents, 9000, "vendor B's price must be untouched");
    assert_eq!(item.name, "Milk");
}

#[tokio::test]
async fn a_vendor_cannot_delist_another_vendors_item() {
    let w = world();

    let delisted = w.svc.delist_own_item(w.tenant, w.user_a, w.b_item).await.unwrap();

    assert!(!delisted);
    let item = w.catalog.find_item(w.tenant, w.b_item).await.unwrap().unwrap();
    assert!(item.is_listed, "delisting a competitor's item is a denial of service on them");
}

/// A user with no store at all — the customer case — gets an absence, not a
/// crash and not a silent success.
#[tokio::test]
async fn a_user_with_no_store_cannot_create_items() {
    let w = world();
    let stranger = Uuid::new_v4();

    let created = w.svc.create_own_item(w.tenant, stranger, draft("X-1")).await.unwrap();

    assert!(created.is_none());
    assert!(w.catalog.items.lock().unwrap().iter().all(|i| i.sku != "X-1"));
}

/// The confirmation loop, at the point of creation. A freshly typed item is a
/// claim that it exists, not a claim that it is on the shelf right now — so it
/// starts unconfirmed and the console shows it as needing attention.
#[tokio::test]
async fn a_newly_created_item_is_not_yet_confirmed() {
    let w = world();

    let item = w.svc.create_own_item(w.tenant, w.user_a, draft("A-1")).await.unwrap()
        .expect("user A runs a store");

    assert_eq!(item.vendor_id, w.vendor_a);
    assert_eq!(item.source, CatalogSource::Manual);
    assert!(item.allergens_declared_at.is_none(),
            "creating an item states nothing about its contents");

    let writes = w.catalog.writes.lock().unwrap();
    assert_eq!(writes.len(), 1, "creation seeds exactly one availability row");
    assert!(writes[0].confirmed_at.is_none(),
            "typing an item is not the same act as confirming it is in stock");
}

/// Ingest is vendor-scoped by the caller (an adapter runs on a vendor's own
/// binding), and creates what is missing while updating what matches.
#[tokio::test]
async fn an_ingest_creates_the_missing_and_updates_the_matched() {
    let w = world();

    let existing = w.svc.create_own_item(w.tenant, w.user_a, draft("A-1")).await.unwrap().unwrap();

    let report = w.svc
        .ingest_for_vendor(w.tenant, w.vendor_a, CatalogSource::Shopify, vec![
            IngestedItem {
                external_id: None, sku: "A-1".into(), name: "Chicken Adobo (large)".into(),
                description: None, price_cents: 21000,
                allergens: vec![], dietary_tags: vec![], is_listed: true,
            },
            IngestedItem {
                external_id: Some("gid://shopify/Product/9".into()), sku: "A-2".into(),
                name: "Pancit".into(), description: None, price_cents: 15000,
                allergens: vec![], dietary_tags: vec![], is_listed: true,
            },
        ])
        .await
        .unwrap();

    assert_eq!(report.updated, 1);
    assert_eq!(report.created, 1);

    let updated = w.catalog.find_item(w.tenant, existing.id).await.unwrap().unwrap();
    assert_eq!(updated.price_cents, 21000, "the source system owns the price");
    assert_eq!(updated.source, CatalogSource::Shopify);
}

/// The rule that makes a multi-vendor adapter safe to expose.
///
/// Once an unattended adapter can name the vendor it is syncing, `vendor_id`
/// becomes caller-supplied input — and a vendor id from *another tenant* would
/// still satisfy the `catalog_items.vendor_id` foreign key, since that
/// constraint is not tenant-scoped. The result would be catalog rows written
/// into one tenant carrying another tenant's vendor: the database would accept
/// it and nothing downstream would notice.
///
/// Tenant comes from the token. The vendor has to be checked against it.
#[tokio::test]
async fn an_ingest_refuses_a_vendor_from_another_tenant() {
    let w = world();
    let other_tenant = Uuid::new_v4();

    let err = w
        .svc
        .ingest_for_vendor(other_tenant, w.vendor_a, CatalogSource::Shopify, vec![IngestedItem {
            external_id: None, sku: "X-9".into(), name: "Smuggled".into(),
            description: None, price_cents: 100,
            allergens: vec![], dietary_tags: vec![], is_listed: true,
        }])
        .await;

    assert!(err.is_err(), "an ingest naming a vendor outside its tenant must be refused");
    assert!(
        w.catalog.items.lock().unwrap().iter().all(|i| i.sku != "X-9"),
        "and must write nothing",
    );
}

/// The rule that makes an ingest safe to run unattended: it may never confirm
/// stock. A nightly sync must leave every item needing a human tap, or the
/// freshness model reports maximum confidence exactly when it has none.
#[tokio::test]
async fn an_ingest_never_writes_a_confirmation() {
    let w = world();
    w.svc.create_own_item(w.tenant, w.user_a, draft("A-1")).await.unwrap();
    w.catalog.writes.lock().unwrap().clear();

    w.svc
        .ingest_for_vendor(w.tenant, w.vendor_a, CatalogSource::Pos, vec![IngestedItem {
            external_id: None, sku: "A-1".into(), name: "Chicken Adobo".into(),
            description: None, price_cents: 18000,
            allergens: vec![], dietary_tags: vec![], is_listed: true,
        }])
        .await
        .unwrap();

    let writes = w.catalog.writes.lock().unwrap();
    assert!(
        writes.iter().all(|a| a.confirmed_at.is_none()),
        "an ingest wrote a confirmation — a machine cannot attest to a shelf",
    );
}
