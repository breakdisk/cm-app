pub mod api;
pub mod application;
pub mod bootstrap;
pub mod config;
pub mod domain;
pub mod infrastructure;
pub mod mcp;

use std::sync::Arc;
use application::services::{CarrierService, MarketplaceService};
use infrastructure::{
    adapters::AdapterRegistry,
    db::PgCarrierBookingRepository,
    storage::StorageAdapter,
};
use logisticos_auth::jwt::JwtService;

#[derive(Clone)]
pub struct AppState {
    pub carrier_svc:      Arc<CarrierService>,
    pub marketplace_svc:  Arc<MarketplaceService>,
    /// JWT validator threaded into the auth middleware. Without this every
    /// handler that pulls `AuthClaims` would 500 with "Auth middleware not
    /// mounted" — the extractor needs `Claims` injected by `require_auth`
    /// upstream of the route.
    pub jwt:              Arc<JwtService>,
    /// 3PL carrier adapters — keyed by carrier code (DHL/FEDEX/UPS/TNT/DPD/ARAMEX).
    /// Only carriers whose credentials are present in config are registered.
    pub adapter_registry: Arc<AdapterRegistry>,
    /// S3/R2 object storage for carrier label files.
    /// None when storage env vars are absent (dev without R2 credentials).
    pub storage:          Option<Arc<dyn StorageAdapter>>,
    /// Append-only audit log for all 3PL carrier API bookings.
    pub booking_repo:     Option<Arc<PgCarrierBookingRepository>>,
}
