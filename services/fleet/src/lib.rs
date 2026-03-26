pub mod api;
pub mod application;
pub mod bootstrap;
pub mod config;
pub mod domain;
pub mod infrastructure;

use std::sync::Arc;
use application::services::FleetService;

#[derive(Clone)]
pub struct AppState {
    pub fleet_svc: Arc<FleetService>,
}
