//! WooCommerce products → OmniDeliv catalog.
//!
//! The sibling of `shopify_catalog`, and the point of the ingest port: the
//! merge rules — what a sync may overwrite, what it may never assert — are not
//! restated here. They live once, in omnideliv's `CatalogItem::merge_ingested`.
//! This file only translates.
//!
//! ## Three places Woo differs from Shopify, all of them load-bearing
//!
//! 1. **Paging is `?page=N`, bounded by the `X-WP-TotalPages` header.** Woo has
//!    no cursor. Reading the header rather than looping until an empty page
//!    matters because Woo returns an *error* past the last page, not an empty
//!    list.
//! 2. **Variable products are not sellable.** A `variable` parent carries a
//!    "from" price and no SKU; the sellable things are its variations, behind a
//!    second endpoint. Mapping the parent would list a product at its cheapest
//!    variant's price — undercharging on every order.
//! 3. **Tags arrive as objects, not a comma-separated string.**

use logisticos_errors::{AppError, AppResult};
use serde::Deserialize;

use crate::adapters::shopify_catalog::IngestItem;
use crate::domain::entities::ConnectorCredentials;

/// Woo's ceiling for `per_page` on this endpoint.
const PAGE_SIZE: u32 = 100;

/// Backstop against a paging bug hammering a merchant's WordPress host, which
/// is usually far more fragile than Shopify's API.
const MAX_PAGES: u32 = 100;

/// Variable products cost one extra request each. A menu with hundreds of them
/// would turn one sync into hundreds of calls against shared hosting, so the
/// fan-out is capped and what was dropped is reported rather than swallowed.
const MAX_VARIABLE_PRODUCTS: usize = 200;

const ALLERGEN_TAG_PREFIX: &str = "allergen:";

#[derive(Debug, Deserialize)]
pub struct Product {
    pub id:      u64,
    pub name:    String,
    #[serde(default)]
    pub sku:     Option<String>,
    #[serde(default)]
    pub price:   Option<String>,
    /// `simple` | `variable` | `grouped` | `external`.
    #[serde(default)]
    pub r#type:  Option<String>,
    /// `publish` | `draft` | `pending` | `private`.
    #[serde(default)]
    pub status:  Option<String>,
    #[serde(default)]
    pub short_description: Option<String>,
    #[serde(default)]
    pub tags:    Vec<Tag>,
}

#[derive(Debug, Deserialize)]
pub struct Tag {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct Variation {
    pub id:    u64,
    #[serde(default)]
    pub sku:   Option<String>,
    #[serde(default)]
    pub price: Option<String>,
    /// Woo describes a variation by its attribute values, not a title.
    #[serde(default)]
    pub attributes: Vec<VariationAttribute>,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct VariationAttribute {
    #[serde(default)]
    pub option: Option<String>,
}

/// What one mapping pass produced, and what it could not.
///
/// The skipped count exists so a sync that quietly dropped half a catalog
/// cannot report success. It is surfaced to the merchant, not just logged.
#[derive(Debug, Default, PartialEq)]
pub struct Mapped {
    pub items: Vec<IngestItem>,
    /// Variable parents whose variations were not fetched — the fan-out cap.
    pub deferred_variable: usize,
    /// Rows with no usable price. A Woo product can genuinely have none.
    pub unpriced: usize,
}

/// Map the directly sellable products. Variable parents are counted, never
/// mapped — see `map_variations` for their real rows.
pub fn map_products(products: &[Product]) -> Mapped {
    let mut out = Mapped::default();

    for p in products {
        if p.r#type.as_deref() == Some("variable") {
            // Not sellable. Its price field is the "from" price of its cheapest
            // variation, and listing that would undercharge every order.
            continue;
        }
        // `grouped` products are containers with no price of their own; they
        // fall out through the price check below rather than needing a case.

        let Some(price_cents) = p.price.as_deref().and_then(parse_price_cents) else {
            out.unpriced += 1;
            continue;
        };

        let (allergens, dietary_tags) = split_tags(&p.tags);
        out.items.push(IngestItem {
            external_id:  Some(p.id.to_string()),
            sku:          fallback_sku(p.sku.as_deref(), p.id),
            name:         p.name.clone(),
            description:  p.short_description.clone().filter(|s| !s.trim().is_empty()),
            price_cents,
            allergens,
            dietary_tags,
            // Only `publish` is on sale. Draft and private products still sync,
            // delisted, so publishing one later needs no re-import.
            is_listed:    p.status.as_deref() == Some("publish"),
        });
    }

    out
}

/// Map one variable product's variations into sellable rows.
pub fn map_variations(parent: &Product, variations: &[Variation]) -> Mapped {
    let mut out = Mapped::default();
    let (allergens, dietary_tags) = split_tags(&parent.tags);
    let parent_listed = parent.status.as_deref() == Some("publish");

    for v in variations {
        let Some(price_cents) = v.price.as_deref().and_then(parse_price_cents) else {
            out.unpriced += 1;
            continue;
        };

        let suffix: Vec<&str> = v
            .attributes
            .iter()
            .filter_map(|a| a.option.as_deref())
            .filter(|o| !o.trim().is_empty())
            .collect();

        out.items.push(IngestItem {
            external_id:  Some(v.id.to_string()),
            sku:          fallback_sku(v.sku.as_deref(), v.id),
            name:         if suffix.is_empty() {
                parent.name.clone()
            } else {
                format!("{} — {}", parent.name, suffix.join(", "))
            },
            description:  parent.short_description.clone().filter(|s| !s.trim().is_empty()),
            price_cents,
            allergens:    allergens.clone(),
            dietary_tags: dietary_tags.clone(),
            // A published variation of an unpublished parent is not on sale.
            // Woo lets those states disagree; the customer sees the parent.
            is_listed:    parent_listed && v.status.as_deref() != Some("private"),
        });
    }

    out
}

/// Woo SKUs are optional exactly as Shopify's are, and omnideliv rejects a
/// blank one — so the same stable fallback, or a merchant loses rows to a
/// `rejected` count with no way to tell which.
fn fallback_sku(sku: Option<&str>, id: u64) -> String {
    match sku.map(str::trim) {
        Some(s) if !s.is_empty() => s.to_owned(),
        _ => format!("woo-{id}"),
    }
}

/// Only an explicit `allergen:` tag is an allergen claim — identical rule to
/// the Shopify adapter, and for the identical reason: a machine must not put
/// words in a vendor's mouth about the field that decides whether we serve a
/// customer with an allergy.
fn split_tags(tags: &[Tag]) -> (Vec<String>, Vec<String>) {
    let mut allergens = Vec::new();
    let mut dietary   = Vec::new();
    for t in tags {
        let lower = t.name.trim().to_lowercase();
        if lower.is_empty() {
            continue;
        }
        match lower.strip_prefix(ALLERGEN_TAG_PREFIX) {
            Some(a) if !a.trim().is_empty() => allergens.push(a.trim().to_owned()),
            _ => dietary.push(lower),
        }
    }
    (allergens, dietary)
}

fn parse_price_cents(price: &str) -> Option<i64> {
    let t = price.trim();
    if t.is_empty() {
        return None;
    }
    t.parse::<f64>().ok().map(|f| (f * 100.0).round() as i64)
}

fn basic_auth(creds: &ConnectorCredentials) -> AppResult<String> {
    use base64::Engine;
    let key = creds.woo_consumer_key().ok_or_else(|| AppError::ExternalService {
        service: "woocommerce".into(),
        message: "consumer_key not configured".into(),
    })?;
    let secret = creds.woo_consumer_secret().ok_or_else(|| AppError::ExternalService {
        service: "woocommerce".into(),
        message: "consumer_secret not configured".into(),
    })?;
    Ok(base64::engine::general_purpose::STANDARD.encode(format!("{key}:{secret}")))
}

/// Read a merchant's whole product catalog, variations included.
pub async fn fetch_products(
    client: &reqwest::Client,
    creds: &ConnectorCredentials,
) -> AppResult<Mapped> {
    let store_url = creds.woo_store_url().ok_or_else(|| AppError::ExternalService {
        service: "woocommerce".into(),
        message: "store_url not configured".into(),
    })?;
    let auth = basic_auth(creds)?;
    let base = store_url.trim_end_matches('/');

    let mut mapped   = Mapped::default();
    let mut variable = Vec::new();
    let mut page     = 1u32;

    loop {
        let url = format!("{base}/wp-json/wc/v3/products?per_page={PAGE_SIZE}&page={page}");
        let resp = get(client, &url, &auth).await?;

        // Woo reports the page count in a header. Looping "until empty" instead
        // walks one page past the end, where Woo answers 400 rather than `[]` —
        // a sync that always ends in an error it then has to ignore.
        let total_pages = resp
            .headers()
            .get("x-wp-totalpages")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(1);

        let products: Vec<Product> = resp.json().await.map_err(|e| AppError::ExternalService {
            service: "woocommerce".into(),
            message: format!("product response was not the expected shape: {e}"),
        })?;

        let batch = map_products(&products);
        mapped.items.extend(batch.items);
        mapped.unpriced += batch.unpriced;

        variable.extend(
            products
                .into_iter()
                .filter(|p| p.r#type.as_deref() == Some("variable")),
        );

        if page >= total_pages || page >= MAX_PAGES {
            if page >= MAX_PAGES && page < total_pages {
                tracing::warn!(store = %base, total_pages, "stopped paging Woo products at the page cap");
            }
            break;
        }
        page += 1;
    }

    // One extra request per variable product. Capped, and the remainder is
    // reported rather than dropped in silence.
    if variable.len() > MAX_VARIABLE_PRODUCTS {
        mapped.deferred_variable = variable.len() - MAX_VARIABLE_PRODUCTS;
        variable.truncate(MAX_VARIABLE_PRODUCTS);
    }

    for parent in &variable {
        let url = format!(
            "{base}/wp-json/wc/v3/products/{}/variations?per_page={PAGE_SIZE}",
            parent.id
        );
        let resp = get(client, &url, &auth).await?;
        let variations: Vec<Variation> = resp.json().await.map_err(|e| AppError::ExternalService {
            service: "woocommerce".into(),
            message: format!("variation response was not the expected shape: {e}"),
        })?;

        let batch = map_variations(parent, &variations);
        mapped.items.extend(batch.items);
        mapped.unpriced += batch.unpriced;
    }

    Ok(mapped)
}

async fn get(client: &reqwest::Client, url: &str, auth: &str) -> AppResult<reqwest::Response> {
    let resp = client
        .get(url)
        .header(reqwest::header::AUTHORIZATION, format!("Basic {auth}"))
        .send()
        .await
        .map_err(|e| AppError::ExternalService {
            service: "woocommerce".into(),
            message: e.to_string(),
        })?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text   = resp.text().await.unwrap_or_default();
        return Err(AppError::ExternalService {
            service: "woocommerce".into(),
            message: format!("product fetch failed: HTTP {status} — {text}"),
        });
    }
    Ok(resp)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn products(json: &str) -> Vec<Product> {
        serde_json::from_str(json).expect("fixture must parse")
    }

    /// The bug this file exists to avoid. A `variable` parent's `price` is its
    /// cheapest variation's price; listing the parent sells the large at the
    /// small's price, on every order, silently.
    #[test]
    fn a_variable_parent_is_never_sold_at_its_from_price() {
        let p = products(r#"[
            {"id":1,"name":"Adobo","type":"variable","status":"publish","price":"180.00","tags":[]},
            {"id":2,"name":"Pancit","type":"simple","status":"publish","price":"150.00","sku":"P-1","tags":[]}
        ]"#);

        let m = map_products(&p);

        assert_eq!(m.items.len(), 1, "only the simple product is sellable here");
        assert_eq!(m.items[0].sku, "P-1");
    }

    /// Its variations are the sellable rows, each at its own price.
    #[test]
    fn variations_become_the_sellable_rows() {
        let parent = &products(
            r#"[{"id":1,"name":"Adobo","type":"variable","status":"publish","tags":[]}]"#,
        )[0];
        let vars: Vec<Variation> = serde_json::from_str(r#"[
            {"id":11,"sku":"A-R","price":"180.00","attributes":[{"option":"Regular"}]},
            {"id":12,"sku":"A-L","price":"240.00","attributes":[{"option":"Large"}]}
        ]"#).unwrap();

        let m = map_variations(parent, &vars);

        assert_eq!(m.items.len(), 2);
        assert_eq!(m.items[0].name, "Adobo — Regular");
        assert_eq!(m.items[0].price_cents, 18000);
        assert_eq!(m.items[1].name, "Adobo — Large");
        assert_eq!(m.items[1].price_cents, 24000);
        assert_eq!(m.items[1].external_id.as_deref(), Some("12"),
                   "the variation, not the parent — a re-sync matches on it");
    }

    /// Woo lets a variation be published under an unpublished parent. The
    /// customer only ever sees the parent, so the parent decides.
    #[test]
    fn a_variation_of_an_unpublished_parent_is_not_on_sale() {
        let parent = &products(
            r#"[{"id":1,"name":"Adobo","type":"variable","status":"draft","tags":[]}]"#,
        )[0];
        let vars: Vec<Variation> = serde_json::from_str(
            r#"[{"id":11,"sku":"A-R","price":"180.00","attributes":[],"status":"publish"}]"#,
        ).unwrap();

        assert!(!map_variations(parent, &vars).items[0].is_listed);
    }

    #[test]
    fn only_published_products_are_listed() {
        for (status, listed) in [("publish", true), ("draft", false), ("private", false)] {
            let p = products(&format!(
                r#"[{{"id":1,"name":"x","type":"simple","status":"{status}","price":"1.00","sku":"S","tags":[]}}]"#
            ));
            assert_eq!(map_products(&p).items[0].is_listed, listed, "status {status}");
        }
    }

    /// A blank SKU must not lose the row — omnideliv rejects blanks, and a
    /// merchant would find items missing with nothing saying which.
    #[test]
    fn a_blank_sku_falls_back_to_a_stable_identifier() {
        let p = products(
            r#"[{"id":7,"name":"x","type":"simple","status":"publish","price":"1.00","sku":"","tags":[]}]"#,
        );
        assert_eq!(map_products(&p).items[0].sku, "woo-7");
    }

    /// An unpriceable product is counted, not silently dropped. A sync that
    /// discarded rows without saying so reports success on a half-empty menu.
    #[test]
    fn products_without_a_usable_price_are_counted_not_swallowed() {
        let p = products(r#"[
            {"id":1,"name":"Grouped","type":"grouped","status":"publish","price":"","tags":[]},
            {"id":2,"name":"Ok","type":"simple","status":"publish","price":"1.00","sku":"S","tags":[]}
        ]"#);

        let m = map_products(&p);
        assert_eq!(m.items.len(), 1);
        assert_eq!(m.unpriced, 1, "the dropped row has to be visible somewhere");
    }

    /// Woo sends tags as objects, and the allergen rule is the same one the
    /// Shopify adapter applies — deliberately, since both feed a port that
    /// refuses to treat either as a declaration.
    #[test]
    fn only_explicitly_prefixed_tags_are_read_as_allergens() {
        let p = products(r#"[{"id":1,"name":"Kare Kare","type":"simple","status":"publish",
            "price":"1.00","sku":"S",
            "tags":[{"name":"Bestseller"},{"name":"allergen:Peanuts"},{"name":"peanut-sauce"}]}]"#);

        let item = &map_products(&p).items[0];
        assert_eq!(item.allergens, vec!["peanuts".to_string()]);
        assert!(item.dietary_tags.contains(&"peanut-sauce".to_string()));
        assert!(!item.dietary_tags.contains(&"allergen:peanuts".to_string()));
    }

    #[test]
    fn prices_are_exact_cents() {
        for (input, expected) in [("180.00", 18000), ("180.10", 18010), ("180.5", 18050), ("0.01", 1)] {
            let p = products(&format!(
                r#"[{{"id":1,"name":"x","type":"simple","status":"publish","price":"{input}","sku":"S","tags":[]}}]"#
            ));
            assert_eq!(map_products(&p).items[0].price_cents, expected, "price {input}");
        }
        assert_eq!(parse_price_cents(""), None);
        assert_eq!(parse_price_cents("  "), None);
    }
}
