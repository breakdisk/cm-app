pub mod api;
pub mod application;
pub mod bootstrap;
pub mod config;
pub mod domain;
pub mod infrastructure;

use std::sync::Arc;
use application::services::CarrierService;
use logisticos_auth::jwt::JwtService;

#[derive(Clone)]
pub struct AppState {
    pub carrier_svc: Arc<CarrierService>,
    /// JWT validator threaded into the auth middleware. Without this every
    /// handler that pulls `AuthClaims` would 500 with "Auth middleware not
    /// mounted" — the extractor needs `Claims` injected by `require_auth`
    /// upstream of the route.
    pub jwt: Arc<JwtService>,
}
