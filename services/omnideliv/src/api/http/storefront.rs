//! A vendor's public, shareable storefront.
//!
//! Unauthenticated by design. This is the page a vendor puts in an Instagram
//! bio, prints on a takeaway counter, or serves on their own domain — so it has
//! to work for someone with no account, no app and no prior relationship with
//! the platform.
//!
//! Mounted under `/v1/omnideliv/public/`, which the API gateway already treats
//! as skipping authentication (the same prefix product photos use). Nothing
//! here reads a principal, because there is never one.
//!
//! ## What it will and will not say
//!
//! It returns exactly what a menu needs: the vendor's name, tagline, address
//! and items. It does **not** return the vendor's id-bearing internals —
//! `commission_bps`, `payout_account`, `user_id` — because this response is
//! world-readable and those are the terms of a commercial contract.
//!
//! An unpublished storefront is a plain 404, indistinguishable from one that
//! does not exist. `public_enabled` is in the repository's WHERE clause rather
//! than checked here, so "this vendor exists but is not published" is not a
//! distinction this endpoint can leak.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::Serialize;
use uuid::Uuid;

use crate::api::http::AppState;

#[derive(Debug, Serialize)]
pub struct PublicItem {
    pub item_id:     Uuid,
    pub name:        String,
    pub price_cents: i64,
    pub category:    Option<String>,
    pub has_photo:   bool,
}

#[derive(Debug, Serialize)]
pub struct PublicStorefront {
    pub vendor_id:   Uuid,
    /// Needed by the client to build public photo URLs, which are tenant-scoped.
    pub tenant_id:   Uuid,
    pub name:        String,
    pub tagline:     Option<String>,
    pub address:     String,
    pub vertical:    String,
    pub slug:        Option<String>,
    /// Whether the store is taking orders at all right now. A menu is worth
    /// showing either way — a closed restaurant still wants its menu findable —
    /// so this is a flag on the response, not a reason to 404.
    pub open:        bool,
    pub items:       Vec<PublicItem>,
}

pub fn public_routes() -> Router<Arc<AppState>> {
    Router::new().route(
        "/v1/omnideliv/public/storefront/:handle",
        get(get_storefront),
    )
}

/// `GET /v1/omnideliv/public/storefront/:handle`
///
/// `handle` is either a slug (`kanto-freestyle`) or a custom domain
/// (`menu.kanto.ph`). One endpoint for both, because to a caller they are the
/// same thing: the public name of a storefront. The landing app's middleware
/// passes whichever it has.
async fn get_storefront(
    State(st): State<Arc<AppState>>,
    Path(handle): Path<String>,
) -> Result<Json<PublicStorefront>, StatusCode> {
    let vendor = st
        .vendors
        .find_public_storefront(&handle)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "public storefront lookup failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Tenant comes from the vendor row this lookup returned, never from the
    // caller — the same re-scoping every tenant-less lookup on this platform
    // does. An empty query lists the whole menu.
    let hits = st
        .catalog
        .search(vendor.tenant_id, vendor.id, "", &[], 200)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, vendor_id = %vendor.id, "public menu load failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let items = hits
        .into_iter()
        // Out-of-stock lines are dropped rather than greyed out: this is a
        // shareable menu, and a permanent link full of things you cannot buy
        // reads as a neglected store.
        // `is_listed` first: a delisted item is one the vendor has taken off
        // sale, and this menu is the most public place it could possibly
        // reappear.
        .filter(|s| s.item_with_availability.item.is_listed)
        .filter(|s| {
            s.item_with_availability.availability.state.as_str() != "out_of_stock"
        })
        .map(|s| {
            let it = s.item_with_availability.item;
            PublicItem {
                item_id:     it.id,
                name:        it.name,
                price_cents: it.price_cents,
                category:    it.category,
                // A flag, not a URL — the client derives the public photo path
                // from (tenant, item), so a moved backing store cannot strand
                // links in a page someone shared months ago.
                has_photo:   it.image_key.is_some(),
            }
        })
        .collect();

    Ok(Json(PublicStorefront {
        vendor_id: vendor.id,
        tenant_id: vendor.tenant_id,
        name:      vendor.name,
        tagline:   vendor.tagline,
        address:   vendor.address,
        vertical:  vendor.vertical.as_str().to_string(),
        slug:      vendor.slug,
        open:      vendor.status == crate::domain::entities::VendorStatus::Active,
        items,
    }))
}
