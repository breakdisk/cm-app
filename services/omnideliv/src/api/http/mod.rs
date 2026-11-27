pub mod baskets;
pub mod catalog;
pub mod courier_jobs;
pub mod health;
pub mod mesh;
pub mod orders;
pub mod scan_limit;
pub mod storefront;
pub mod tables;
pub mod tracking;
pub mod vendor_orders;
pub mod vendors;
pub mod venues;

use std::sync::Arc;
use axum::Router;

use crate::application::services::{BasketService, CatalogService, CheckoutService};
use crate::domain::repositories::{
    OrderRepository, TelemetryRepository, VendorLedgerRepository, VendorLegRepository,
    VendorRepository,
};
use omnideliv_mesh::MeshRunner;

pub struct AppState {
    pub catalog: Arc<CatalogService>,
    pub baskets: Arc<BasketService>,
    pub mesh:    Arc<MeshRunner>,
    pub checkout: Arc<CheckoutService>,
    pub orders:    Arc<dyn OrderRepository>,
    pub telemetry: Arc<dyn TelemetryRepository>,
    pub ledgers:   Arc<dyn VendorLedgerRepository>,
    pub order_events: Arc<dyn crate::infrastructure::messaging::OrderEvents>,
    pub jwt:     Arc<logisticos_auth::jwt::JwtService>,
    /// `None` when storage is unconfigured. The photo routes report that
    /// plainly rather than the service refusing to boot — a catalog without
    /// pictures is still a catalog.
    pub photos:  Option<Arc<crate::infrastructure::storage::PhotoStorage>>,
    /// Where the courier is. Distinct from `telemetry` above, which is the
    /// order event log.
    pub courier_telemetry: Arc<dyn crate::application::services::CourierTelemetry>,
    pub vendors:           Arc<dyn VendorRepository>,
    /// One leg, moved conditionally. Distinct from `orders` above, which writes
    /// a whole order last-write-wins — correct for a checkout, wrong for a
    /// transition two tablets may attempt at once.
    pub legs:              Arc<dyn VendorLegRepository>,
    pub vendor_events:     Arc<dyn crate::infrastructure::messaging::VendorLegEvents>,
    pub venues:            Arc<dyn crate::domain::repositories::VenueRepository>,
    /// How long a scanned table session lives. Minutes, not hours — the
    /// credential that mints it is printed on vinyl in a public room.
    pub table_session_mins: i64,
    /// Live sessions one table may hold at once, so a photographed code is not
    /// an unbounded session factory.
    pub table_session_cap:  i64,
    pub table_scan_base_url: String,
    /// See `Config::online_payment_enabled`. Surfaced to the diner client so a
    /// payment option that cannot work is never offered.
    pub online_payment_enabled: bool,
    /// Bounds the unauthenticated scan endpoint. Per-process — see the
    /// module docs for what that does and does not buy.
    pub scan_limiter:        Arc<scan_limit::ScanLimiter>,
}

pub fn router(state: Arc<AppState>) -> Router {
    // Health is mounted outside the auth layer on purpose: container
    // healthchecks `curl -sf` these, and a 401 reads as a dead service. An open
    // incident on this platform has eight services showing red for eleven days
    // for exactly that reason.
    let auth_layer = axum::middleware::from_fn_with_state(
        Arc::clone(&state.jwt),
        logisticos_auth::middleware::require_auth,
    );

    Router::new()
        .merge(health::routes())
        // Product photos are read by <img> tags with no Authorization header —
        // see catalog::public_routes. Mounted before the auth layer for the
        // same reason health is.
        .merge(catalog::public_routes().with_state(Arc::clone(&state)))
        // A diner scanning a table has no account and never will —
        // mounted outside the auth layer for the same reason as the two above.
        .merge(tables::public_routes().with_state(Arc::clone(&state)))
        // A vendor's shareable public menu. Unauthenticated for the same
        // reason as the three above: its whole purpose is to work for
        // someone with no account who followed a link.
        .merge(storefront::public_routes().with_state(Arc::clone(&state)))
        .merge(
            catalog::routes()
                .merge(baskets::routes())
                .merge(mesh::routes())
                .merge(orders::routes())
                .merge(tracking::routes())
                .merge(vendors::routes())
                .merge(vendor_orders::routes())
                .merge(tables::routes())
                .merge(venues::routes())
                .merge(courier_jobs::routes())
                .layer(auth_layer)
                .with_state(state),
        )
}
