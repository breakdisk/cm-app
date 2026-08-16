//! Shopify products → OmniDeliv catalog.
//!
//! The second half of the Shopify integration. `shopify.rs` takes orders *out*
//! of a merchant's store; this puts their menu *in*. Both are translation
//! layers and neither owns a rule: the merge rules — what a sync may overwrite,
//! what it may never assert — live in omnideliv's `CatalogItem::merge_ingested`,
//! so a third and fourth adapter cannot each invent their own.
//!
//! ## Why this defines its own `IngestItem`
//!
//! It is byte-identical to `logisticos_omnideliv::domain::entities::IngestedItem`
//! and deliberately not imported from it. Depending on another service's crate
//! for a DTO couples two independently deployable services at compile time to
//! share what is really an HTTP contract; the next schema change would then have
//! to land in both at once. The duplication is the point — the wire format is
//! the contract, and it is pinned by a test here.
//!
//! ## Everything that decides anything is in `map_products`
//!
//! It is a pure function over parsed JSON: no network, no clock, no database.
//! The client below is deliberately dumb, because a mapping bug prices a
//! merchant's whole catalog wrongly and that has to be testable without a
//! Shopify store.

use logisticos_errors::{AppError, AppResult};
use serde::{Deserialize, Serialize};

use crate::domain::entities::ConnectorCredentials;

/// One sellable thing, in the shape omnideliv's ingest port accepts.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct IngestItem {
    pub external_id:  Option<String>,
    pub sku:          String,
    pub name:         String,
    pub description:  Option<String>,
    pub price_cents:  i64,
    pub allergens:    Vec<String>,
    pub dietary_tags: Vec<String>,
    pub is_listed:    bool,
}

#[derive(Debug, Deserialize)]
pub struct ProductsResponse {
    #[serde(default)]
    pub products: Vec<Product>,
}

#[derive(Debug, Deserialize)]
pub struct Product {
    pub id:        u64,
    pub title:     String,
    #[serde(default)]
    pub body_html: Option<String>,
    /// `active` | `draft` | `archived`.
    #[serde(default)]
    pub status:    Option<String>,
    /// Shopify sends this as one comma-separated string, not an array.
    #[serde(default)]
    pub tags:      String,
    #[serde(default)]
    pub variants:  Vec<Variant>,
}

#[derive(Debug, Deserialize)]
pub struct Variant {
    pub id:    u64,
    #[serde(default)]
    pub sku:   Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    /// A decimal string — `"180.00"`, sometimes `"180.5"`.
    #[serde(default)]
    pub price: Option<String>,
}

/// Shopify's sentinel for "this product has no real variants".
const DEFAULT_VARIANT: &str = "Default Title";

/// The prefix that makes a tag an allergen claim.
///
/// Only `allergen:peanuts` becomes an allergen. A bare `peanuts` tag does not,
/// and neither does anything clever inferred from the title. Guessing here
/// would put a machine's opinion where a person's declaration belongs — and
/// omnideliv would still refuse to treat it as one, so the guess would buy
/// nothing and cost a wrong answer.
const ALLERGEN_TAG_PREFIX: &str = "allergen:";

pub fn map_products(resp: &ProductsResponse) -> Vec<IngestItem> {
    let mut out = Vec::new();

    for p in &resp.products {
        let (allergens, dietary_tags) = split_tags(&p.tags);
        let is_listed = p.status.as_deref() == Some("active");
        let description = p.body_html.clone().filter(|s| !s.trim().is_empty());

        for v in &p.variants {
            let Some(price_cents) = v.price.as_deref().and_then(parse_price_cents) else {
                continue;
            };

            out.push(IngestItem {
                external_id:  Some(v.id.to_string()),
                // Shopify treats a variant SKU as optional and merchants leave
                // it blank constantly. omnideliv rejects a blank one, so
                // passing it through would drop those items with a `rejected`
                // count and no way to tell which ones. Derived from the variant
                // id: stable, so the next sync matches the same row.
                sku: match v.sku.as_deref().map(str::trim) {
                    Some(s) if !s.is_empty() => s.to_owned(),
                    _ => format!("shopify-{}", v.id),
                },
                name:         variant_name(&p.title, v.title.as_deref()),
                description:  description.clone(),
                price_cents,
                allergens:    allergens.clone(),
                dietary_tags: dietary_tags.clone(),
                is_listed,
            });
        }
    }

    out
}

/// "Chicken Adobo" + "Large" → "Chicken Adobo — Large".
///
/// Without the variant name a merchant with three sizes gets three identical
/// rows in the console and no way to tell which one they are marking out of
/// stock. `Default Title` is Shopify's sentinel for a product that has no real
/// variants and must never be shown to anyone.
fn variant_name(product_title: &str, variant_title: Option<&str>) -> String {
    match variant_title.map(str::trim) {
        Some(t) if !t.is_empty() && t != DEFAULT_VARIANT => format!("{product_title} — {t}"),
        _ => product_title.to_owned(),
    }
}

fn split_tags(tags: &str) -> (Vec<String>, Vec<String>) {
    let mut allergens = Vec::new();
    let mut dietary   = Vec::new();
    for t in tags.split(',').map(str::trim).filter(|t| !t.is_empty()) {
        let lower = t.to_lowercase();
        match lower.strip_prefix(ALLERGEN_TAG_PREFIX) {
            Some(a) if !a.trim().is_empty() => allergens.push(a.trim().to_owned()),
            _ => dietary.push(lower),
        }
    }
    (allergens, dietary)
}

/// `"180.10"` → `18010`.
///
/// f64 with an explicit `round()`, matching `shopify.rs`. The intermediate is
/// inexact — 180.10 × 100 is 18009.999… — and `round()` is what makes it exact
/// again for any two-decimal value, which is every price Shopify will send.
/// The tests pin that across the range rather than leaving it to be trusted.
fn parse_price_cents(price: &str) -> Option<i64> {
    price.trim().parse::<f64>().ok().map(|f| (f * 100.0).round() as i64)
}

/// Page size. Shopify's ceiling for this endpoint is 250, and a menu is small
/// enough that a merchant with 300 products should cost two round trips rather
/// than twelve.
const PAGE_SIZE: u32 = 250;

/// A stop that exists so a paging bug cannot become an infinite loop against
/// someone else's API. 100 pages is 25,000 variants — far past any storefront
/// this product serves, and reached only if Shopify's cursor stops advancing.
const MAX_PAGES: usize = 100;

/// Read a merchant's whole product catalog.
///
/// Cursor-paginated through the `Link` header, which is how Shopify has paged
/// since 2019 — `page=N` was removed and silently returns page 1 forever, so a
/// naive implementation would sync the first 250 products over and over and
/// look like it worked.
pub async fn fetch_products(
    client: &reqwest::Client,
    creds: &ConnectorCredentials,
) -> AppResult<Vec<IngestItem>> {
    let shop_domain = creds.shopify_shop_domain().ok_or_else(|| AppError::ExternalService {
        service: "shopify".into(),
        message: "shop_domain not configured in connector credentials".into(),
    })?;
    let admin_token = creds.shopify_admin_token().ok_or_else(|| AppError::ExternalService {
        service: "shopify".into(),
        message: "admin_api_token not configured in connector credentials".into(),
    })?;

    let mut url = Some(format!(
        "https://{shop_domain}/admin/api/2024-01/products.json?limit={PAGE_SIZE}"
    ));
    let mut out   = Vec::new();
    let mut pages = 0usize;

    while let Some(next) = url.take() {
        pages += 1;
        if pages > MAX_PAGES {
            tracing::warn!(shop = %shop_domain, "stopped paging Shopify products at the page cap");
            break;
        }

        let resp = client
            .get(&next)
            .header("X-Shopify-Access-Token", admin_token)
            .send()
            .await
            .map_err(|e| AppError::ExternalService {
                service: "shopify".into(),
                message: e.to_string(),
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text   = resp.text().await.unwrap_or_default();
            return Err(AppError::ExternalService {
                service: "shopify".into(),
                message: format!("product fetch failed: HTTP {status} — {text}"),
            });
        }

        // Read the cursor before consuming the body.
        url = resp
            .headers()
            .get(reqwest::header::LINK)
            .and_then(|v| v.to_str().ok())
            .and_then(next_page_url);

        let body: ProductsResponse = resp.json().await.map_err(|e| AppError::ExternalService {
            service: "shopify".into(),
            message: format!("product response was not the expected shape: {e}"),
        })?;

        out.extend(map_products(&body));
    }

    Ok(out)
}

/// Pull `<https://…>; rel="next"` out of a `Link` header.
///
/// Shopify sends `prev` and `next` in one header, in either order, so matching
/// on `rel="next"` rather than taking the first or last URL is load-bearing:
/// taking the wrong one pages backwards forever.
fn next_page_url(link_header: &str) -> Option<String> {
    link_header.split(',').find_map(|part| {
        if !part.contains(r#"rel="next""#) {
            return None;
        }
        let start = part.find('<')? + 1;
        let end   = part[start..].find('>')? + start;
        Some(part[start..end].to_owned())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> ProductsResponse {
        serde_json::from_str(json).expect("fixture must parse")
    }

    /// The unit Shopify sells is the *variant*, not the product. A product with
    /// a regular and a large size is two things with two SKUs and two prices;
    /// collapsing them to one row loses the price difference and leaves half
    /// the merchant's menu unsellable.
    #[test]
    fn each_variant_becomes_its_own_item() {
        let r = parse(r#"{"products":[{
            "id": 1, "title": "Chicken Adobo", "status": "active", "tags": "",
            "variants": [
                {"id": 11, "sku": "ADOBO-R", "title": "Regular", "price": "180.00"},
                {"id": 12, "sku": "ADOBO-L", "title": "Large",   "price": "240.00"}
            ]}]}"#);

        let items = map_products(&r);

        assert_eq!(items.len(), 2, "two variants are two sellable items");
        assert_eq!(items[0].sku, "ADOBO-R");
        assert_eq!(items[0].price_cents, 18000);
        assert_eq!(items[1].sku, "ADOBO-L");
        assert_eq!(items[1].price_cents, 24000);
    }

    /// The external id must be the variant, not the product — it is the key a
    /// re-sync matches on, and two variants sharing a product id would make the
    /// second overwrite the first on every run.
    #[test]
    fn the_external_id_identifies_the_variant_not_the_product() {
        let r = parse(r#"{"products":[{
            "id": 1, "title": "Adobo", "status": "active", "tags": "",
            "variants": [
                {"id": 11, "sku": "A", "title": "Regular", "price": "1.00"},
                {"id": 12, "sku": "B", "title": "Large",   "price": "2.00"}
            ]}]}"#);

        let items = map_products(&r);
        assert_eq!(items[0].external_id.as_deref(), Some("11"));
        assert_eq!(items[1].external_id.as_deref(), Some("12"));
    }

    /// A named variant belongs in the name, or a merchant's console shows two
    /// rows called "Chicken Adobo" and no way to tell which is the large one.
    #[test]
    fn a_named_variant_is_part_of_the_item_name() {
        let r = parse(r#"{"products":[{
            "id": 1, "title": "Chicken Adobo", "status": "active", "tags": "",
            "variants": [{"id": 11, "sku": "A", "title": "Large", "price": "240.00"}]}]}"#);

        assert_eq!(map_products(&r)[0].name, "Chicken Adobo — Large");
    }

    /// ...but Shopify names the sole variant of a simple product
    /// "Default Title", which must never reach a customer.
    #[test]
    fn the_default_variant_sentinel_never_reaches_the_name() {
        let r = parse(r#"{"products":[{
            "id": 1, "title": "Chicken Adobo", "status": "active", "tags": "",
            "variants": [{"id": 11, "sku": "A", "title": "Default Title", "price": "180.00"}]}]}"#);

        assert_eq!(map_products(&r)[0].name, "Chicken Adobo");
    }

    /// Shopify variant SKUs are frequently blank — it is an optional field in
    /// their admin. omnideliv rejects a blank SKU, so a passthrough would drop
    /// those items silently and a merchant would find half their menu missing
    /// with nothing in any log saying why.
    #[test]
    fn a_blank_sku_falls_back_to_a_stable_identifier() {
        let r = parse(r#"{"products":[{
            "id": 1, "title": "Adobo", "status": "active", "tags": "",
            "variants": [{"id": 11, "sku": "", "title": "Default Title", "price": "180.00"}]}]}"#);

        let items = map_products(&r);
        assert_eq!(items.len(), 1, "a blank SKU must not lose the item");
        assert_eq!(items[0].sku, "shopify-11",
                   "stable and derived from the variant, so a re-sync matches it again");
    }

    /// Money. `"180.10"` through f64 is 18009.999…, and a catalog that is a
    /// cent light on every third item is the kind of bug nobody reports and
    /// everybody pays for.
    #[test]
    fn prices_are_exact_cents_not_floating_point() {
        for (input, expected) in [
            ("180.00", 18000),
            ("180.10", 18010),
            ("180.5",  18050),
            ("0.01",   1),
            ("1999.99", 199_999),
            ("180",    18000),
        ] {
            let r = parse(&format!(
                r#"{{"products":[{{"id":1,"title":"x","status":"active","tags":"",
                    "variants":[{{"id":11,"sku":"S","title":"Default Title","price":"{input}"}}]}}]}}"#
            ));
            assert_eq!(map_products(&r)[0].price_cents, expected, "price {input}");
        }
    }

    /// Draft and archived products exist in Shopify but are not for sale. They
    /// still sync — a merchant who publishes one later should not have to
    /// re-import — but they arrive delisted.
    #[test]
    fn unpublished_products_arrive_delisted() {
        for (status, listed) in [("active", true), ("draft", false), ("archived", false)] {
            let r = parse(&format!(
                r#"{{"products":[{{"id":1,"title":"x","status":"{status}","tags":"",
                    "variants":[{{"id":11,"sku":"S","title":"Default Title","price":"1.00"}}]}}]}}"#
            ));
            assert_eq!(map_products(&r)[0].is_listed, listed, "status {status}");
        }
    }

    /// Only an explicit `allergen:` tag is an allergen claim. A `peanut-sauce`
    /// product tag is marketing copy, and reading it as an allergen declaration
    /// would be a machine putting words in a vendor's mouth about the one field
    /// that decides whether we serve a customer with an allergy.
    #[test]
    fn only_explicitly_prefixed_tags_are_read_as_allergens() {
        let r = parse(r#"{"products":[{
            "id": 1, "title": "Kare Kare", "status": "active",
            "tags": "Bestseller, allergen:Peanuts, peanut-sauce, halal",
            "variants": [{"id": 11, "sku": "S", "title": "Default Title", "price": "1.00"}]}]}"#);

        let item = &map_products(&r)[0];
        assert_eq!(item.allergens, vec!["peanuts".to_string()],
                   "normalised, and only the prefixed one");
        assert!(item.dietary_tags.contains(&"peanut-sauce".to_string()),
                "an unprefixed tag stays a tag");
        assert!(item.dietary_tags.contains(&"halal".to_string()));
        assert!(!item.dietary_tags.contains(&"allergen:peanuts".to_string()),
                "an allergen tag must not also be sold as a dietary claim");
    }

    /// Shopify puts `prev` and `next` in one header and does not promise an
    /// order. Taking the first URL rather than the one tagged `next` pages
    /// backwards forever — a sync that never terminates and never errors.
    #[test]
    fn the_next_cursor_is_chosen_by_its_rel_not_its_position() {
        let h = concat!(
            r#"<https://x.myshopify.com/admin/api/2024-01/products.json?page_info=PREV>; rel="previous", "#,
            r#"<https://x.myshopify.com/admin/api/2024-01/products.json?page_info=NEXT>; rel="next""#,
        );
        assert_eq!(
            next_page_url(h).as_deref(),
            Some("https://x.myshopify.com/admin/api/2024-01/products.json?page_info=NEXT"),
        );

        // Same header, reversed — `next` first this time.
        let reversed = concat!(
            r#"<https://x.myshopify.com/admin/api/2024-01/products.json?page_info=NEXT>; rel="next", "#,
            r#"<https://x.myshopify.com/admin/api/2024-01/products.json?page_info=PREV>; rel="previous""#,
        );
        assert_eq!(
            next_page_url(reversed).as_deref(),
            Some("https://x.myshopify.com/admin/api/2024-01/products.json?page_info=NEXT"),
        );
    }

    /// The last page carries only a `previous` link. Reading a next cursor out
    /// of that is how a sync loops on its final page.
    #[test]
    fn the_last_page_yields_no_next_cursor() {
        let h = r#"<https://x.myshopify.com/admin/api/2024-01/products.json?page_info=PREV>; rel="previous""#;
        assert_eq!(next_page_url(h), None);
        assert_eq!(next_page_url(""), None);
    }

    /// The wire contract with omnideliv, pinned. This struct is duplicated
    /// rather than imported, so the only thing keeping the two in step is that
    /// the JSON matches — which makes it worth asserting rather than assuming.
    #[test]
    fn the_serialised_shape_matches_omnidelivs_ingest_contract() {
        let item = IngestItem {
            external_id: Some("11".into()),
            sku: "S".into(),
            name: "Adobo".into(),
            description: None,
            price_cents: 18000,
            allergens: vec![],
            dietary_tags: vec![],
            is_listed: true,
        };

        let v = serde_json::to_value(&item).unwrap();
        for field in [
            "external_id", "sku", "name", "description",
            "price_cents", "allergens", "dietary_tags", "is_listed",
        ] {
            assert!(v.get(field).is_some(), "ingest contract is missing `{field}`");
        }
        assert_eq!(v.as_object().unwrap().len(), 8, "no extra fields on the wire");
    }
}
