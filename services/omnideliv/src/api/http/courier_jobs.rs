//! The manifest a courier works a job from.
//!
//! The second half of the split-at-the-claim contract. Before the claim,
//! field-ops returns a thin opaque offer card carrying no addresses at all —
//! `offer_to_nearest` fans out, so anything on the offer reaches every courier
//! merely *considered* for the job. Everything here is disclosed only to
//! whoever actually took responsibility for it.
//!
//! Read live on every open and on the app's adaptive poll, never cached as
//! truth, so a route the Logistics agent rewrites mid-trip simply appears.

use std::sync::Arc;

use axum::{extract::{Path, State}, http::StatusCode, routing::{get, post}, Json, Router};
use logisticos_auth::middleware::AuthClaims;
use serde::Serialize;
use uuid::Uuid;

use crate::api::http::AppState;
use crate::domain::entities::LegStatus;

/// May this caller read this order's manifest?
///
/// A pure function so the rule is testable without a database, and so the
/// fall-open case has a test of its own rather than living in a branch nobody
/// exercises.
fn may_read_manifest(order_courier_user_id: Option<Uuid>, caller: Uuid) -> bool {
    matches!(order_courier_user_id, Some(c) if c == caller)
}

#[derive(Debug, Serialize)]
pub struct ManifestLine {
    pub qty:       i32,
    pub item_name: String,
    /// The chosen options, so a courier can check the bag against what was
    /// ordered — "no ice", "large" — rather than only a product name.
    pub modifiers: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ManifestStop {
    /// What the app sends back on `arrived` and `collected`. Opaque to
    /// field-ops, which never resolves it.
    pub stop_ref:          Uuid,
    pub seq:               i32,
    pub vendor_name:       String,
    pub address:           String,
    pub lat:               f64,
    pub lng:               f64,
    /// Drives the handling copy on the stop card — an ID check for pharmacy,
    /// keep-upright for a florist. field-ops cannot know this, which is the
    /// whole reason the manifest comes from here.
    pub vertical:          String,
    pub prep_time_minutes: i32,
    pub picked_up:         bool,
    pub lines:             Vec<ManifestLine>,
}

#[derive(Debug, Serialize)]
pub struct Dropoff {
    /// The dropoff's `stop_ref` is the order id — pickups use the vendor id.
    /// Both are opaque to field-ops; only this service knows the difference.
    pub stop_ref:       Uuid,
    pub lat:            f64,
    pub lng:            f64,
    /// `None` for orders placed before migration 0019. The app renders the
    /// dropoff without a name rather than inventing one.
    pub customer_name:  Option<String>,
    pub customer_phone: Option<String>,
    /// Always `None` today. A free-text delivery note ("unit 12B, gate code
    /// 4417") needs a field on the customer's checkout screen, which is
    /// customer-app work. Present in the contract so adding it later is not a
    /// breaking change for the app.
    pub notes:          Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ManifestResponse {
    pub order_id:         Uuid,
    pub status:           String,
    /// Cash to collect at the door. 0 for a prepaid order, once that rail
    /// exists.
    pub cod_amount_cents: i64,
    pub trip_cents:       i64,
    pub tip_cents:        i64,
    pub stops:            Vec<ManifestStop>,
    pub dropoff:          Dropoff,
}

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/omnideliv/courier/jobs/:order_id", get(manifest))
        // A separate path, not another method on the one above: two `.route()`
        // calls on the *same* path panic at startup in axum.
        .route("/v1/omnideliv/courier/jobs/:order_id/proof", post(upload_proof))
}

/// `POST /v1/omnideliv/courier/jobs/:order_id/proof` — multipart, field `file`.
///
/// The delivery photo. Until this existed the app captured a proof, encoded it
/// and queued it with nowhere to send it — evidence that died on the phone.
///
/// Bytes go through the service rather than a presigned URL, for the same reason
/// the catalog photos do: the bucket is `minio` on the internal network with no
/// published port and no Traefik route, so a presigned URL points somewhere a
/// courier's phone cannot reach.
///
/// Authorised by the same rule as the manifest read, and every refusal is a 404
/// — a courier must not be able to discover which orders exist, or attach
/// evidence to somebody else's delivery.
async fn upload_proof(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Path(order_id): Path<Uuid>,
    mut multipart: axum::extract::Multipart,
) -> Result<StatusCode, (StatusCode, String)> {
    let storage = st.photos.clone().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "photo storage is not configured".to_string(),
    ))?;

    let order = st
        .orders
        .find_by_id(claims.tenant_id, order_id)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, %order_id, "proof order lookup failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "lookup failed".to_string())
        })?
        .ok_or((StatusCode::NOT_FOUND, "no such job".to_string()))?;

    if !may_read_manifest(order.courier_user_id, claims.user_id) {
        // 404, not 403 — see the module note.
        return Err((StatusCode::NOT_FOUND, "no such job".to_string()));
    }

    let mut bytes: Option<Vec<u8>> = None;
    while let Some(field) = multipart.next_field().await.map_err(|e| {
        (StatusCode::BAD_REQUEST, format!("could not read the upload: {e}"))
    })? {
        if field.name() == Some("file") {
            let data = field.bytes().await.map_err(|e| {
                (StatusCode::BAD_REQUEST, format!("could not read the file: {e}"))
            })?;
            bytes = Some(data.to_vec());
        }
    }

    let bytes = bytes.ok_or((
        StatusCode::BAD_REQUEST,
        "expected a multipart field named 'file'".to_string(),
    ))?;

    if bytes.len() > crate::infrastructure::storage::MAX_PHOTO_BYTES {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            format!("proof is {} KB; the ceiling is {} KB",
                bytes.len() / 1024,
                crate::infrastructure::storage::MAX_PHOTO_BYTES / 1024),
        ));
    }

    // Sniffed from the bytes, never taken from the caller's Content-Type, which
    // is a claim. The app sends WebP; the sniffer accepts JPEG and PNG too,
    // because a courier on a device where the encoder failed still has evidence
    // worth keeping.
    let content_type = crate::infrastructure::storage::sniff_image(&bytes).ok_or((
        StatusCode::UNSUPPORTED_MEDIA_TYPE,
        "that file is not a JPEG, PNG or WebP".to_string(),
    ))?;

    // Tenant-prefixed so one tenant's keys cannot collide with another's, and
    // keyed on the order so a re-upload replaces rather than accumulates —
    // an at-least-once queue will send the same proof twice.
    let key = format!("{}/proofs/{}.webp", claims.tenant_id, order_id);

    storage.put(&key, bytes, content_type).await.map_err(|e| {
        tracing::error!(err = %e, %order_id, "proof upload failed");
        (StatusCode::INTERNAL_SERVER_ERROR, "could not store the proof".to_string())
    })?;

    tracing::info!(%order_id, courier = %claims.user_id, %key, "delivery proof stored");
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /v1/omnideliv/courier/jobs/:order_id`
///
/// Every refusal is a 404, never a 403. Assignment and order ids reach
/// couriers' phones, so a distinguishable "forbidden" would let one courier
/// enumerate which orders exist.
async fn manifest(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Path(order_id): Path<Uuid>,
) -> Result<Json<ManifestResponse>, StatusCode> {
    let order = st
        .orders
        .find_by_id(claims.tenant_id, order_id)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, %order_id, "manifest order lookup failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    if !may_read_manifest(order.courier_user_id, claims.user_id) {
        return Err(StatusCode::NOT_FOUND);
    }

    // Pre-0013 orders carry no destination. 409 rather than a guessed point:
    // sending a courier to the wrong address is worse than telling them this
    // job cannot be worked from the app.
    let (Some(lat), Some(lng)) = (order.delivery_lat, order.delivery_lng) else {
        tracing::warn!(%order_id, "manifest requested for an order with no destination");
        return Err(StatusCode::CONFLICT);
    };

    let vendor_ids: Vec<Uuid> = order.legs.iter().map(|l| l.vendor_id).collect();
    let vendors = st
        .vendors
        .find_by_ids(claims.tenant_id, &vendor_ids)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, %order_id, "manifest vendor lookup failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Line items live on the basket, not the order — the order carries money
    // per vendor, not what was bought. A basket that has since been deleted
    // leaves the stops without contents rather than failing the whole manifest:
    // a courier with addresses and no item list can still work the job.
    let basket = st
        .baskets
        .get(claims.tenant_id, order.basket_id)
        .await
        .unwrap_or_else(|e| {
            tracing::error!(err = %e, %order_id, "manifest basket lookup failed; stops will omit lines");
            None
        });

    let item_ids: Vec<Uuid> = basket
        .as_ref()
        .map(|b| b.lines.iter().map(|l| l.item_id).collect())
        .unwrap_or_default();

    let items = if item_ids.is_empty() {
        Vec::new()
    } else {
        st.catalog
            .find_items(claims.tenant_id, &item_ids)
            .await
            .unwrap_or_else(|e| {
                tracing::error!(err = %e, %order_id, "manifest item lookup failed; lines will be unnamed");
                Vec::new()
            })
    };

    let stops: Vec<ManifestStop> = order
        .legs
        .iter()
        .enumerate()
        .filter_map(|(i, leg)| {
            let v = vendors.iter().find(|v| v.id == leg.vendor_id)?;

            let lines: Vec<ManifestLine> = basket
                .as_ref()
                .map(|b| {
                    b.lines
                        .iter()
                        .filter(|l| l.vendor_id == leg.vendor_id)
                        .map(|l| ManifestLine {
                            qty: l.qty,
                            item_name: items
                                .iter()
                                .find(|it| it.id == l.item_id)
                                .map(|it| it.name.clone())
                                // An item deleted from the catalog after the
                                // order was placed. Naming it plainly beats an
                                // empty string a courier would read as a bug.
                                .unwrap_or_else(|| "Item no longer in catalog".to_string()),
                            modifiers: l
                                .modifiers
                                .iter()
                                .map(|m| format!("{}: {}", m.group_name, m.option_name))
                                .collect(),
                        })
                        .collect()
                })
                .unwrap_or_default();

            Some(ManifestStop {
                stop_ref:          leg.vendor_id,
                seq:               i as i32 + 1,
                vendor_name:       v.name.clone(),
                address:           v.address.clone(),
                lat:               v.lat,
                lng:               v.lng,
                vertical:          v.vertical.as_str().to_string(),
                prep_time_minutes: v.prep_time_minutes,
                picked_up:         leg.status == LegStatus::PickedUp,
                lines,
            })
        })
        .collect();

    Ok(Json(ManifestResponse {
        order_id:         order.id,
        status:           order.status.as_str().to_string(),
        // The customer pays the whole thing at the door — OmniDeliv is COD.
        cod_amount_cents: order.grand_total_cents,
        trip_cents:       order.courier_trip_cents,
        tip_cents:        order.tip_cents,
        stops,
        dropoff: Dropoff {
            stop_ref:       order.id,
            lat,
            lng,
            customer_name:  order.customer_name.clone(),
            customer_phone: order.customer_phone.clone(),
            notes:          order.delivery_note.clone(),
        },
    }))
}

#[cfg(test)]
mod authorization {
    use super::*;

    /// The whole access rule. Order ids reach couriers' phones, so a manifest
    /// keyed on anything a caller can name is a manifest any courier can read.
    #[test]
    fn only_the_carrying_courier_may_read_a_manifest() {
        let carrying = Uuid::new_v4();
        assert!(may_read_manifest(Some(carrying), carrying));
        assert!(!may_read_manifest(Some(carrying), Uuid::new_v4()));
    }

    /// Orders claimed before migration 0020 have no courier recorded. Refuse
    /// rather than fall open — "we do not know who is carrying this" must never
    /// read as "anyone may look".
    #[test]
    fn an_order_with_no_recorded_courier_is_refused() {
        assert!(!may_read_manifest(None, Uuid::new_v4()));
        assert!(!may_read_manifest(None, Uuid::nil()));
    }

    /// The nil uuid is a real value a bug can produce — an unset field, a
    /// default-constructed claim. It must not match an order whose courier is
    /// genuinely unknown.
    #[test]
    fn a_nil_caller_does_not_match_an_unknown_courier() {
        assert!(!may_read_manifest(None, Uuid::nil()));
        assert!(may_read_manifest(Some(Uuid::nil()), Uuid::nil()),
                "but a real nil-id courier still matches itself — the rule is equality, not truthiness");
    }
}
