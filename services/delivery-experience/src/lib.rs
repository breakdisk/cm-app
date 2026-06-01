pub mod api;
pub mod application;
pub mod bootstrap;
pub mod config;
pub mod domain;
pub mod infrastructure;

use std::sync::Arc;
use application::services::TrackingService;
use infrastructure::external::PodClient;

#[derive(Clone)]
pub struct AppState {
    pub tracking_svc: Arc<TrackingService>,
    pub pod_client:   Arc<PodClient>,
}
