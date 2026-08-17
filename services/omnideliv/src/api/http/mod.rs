pub mod baskets;
pub mod catalog;
pub mod courier_jobs;
pub mod health;
pub mod mesh;
pub mod orders;
pub mod tracking;
pub mod vendors;

use std::sync::Arc;
use axum::Router;

use crate::application::services::{BasketService, CatalogService, CheckoutService};
use crate::domain::repositories::{
    OrderRepository, TelemetryRepository, VendorLedgerRepository, VendorRepository,
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
        .merge(
            catalog::routes()
                .merge(baskets::routes())
                .merge(mesh::routes())
                .merge(orders::routes())
                .merge(tracking::routes())
                .merge(vendors::routes())
                .merge(courier_jobs::routes())
                .layer(auth_layer)
                .with_state(state),
        )
}
