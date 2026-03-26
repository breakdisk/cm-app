pub mod api;
pub mod application;
pub mod bootstrap;
pub mod config;
pub mod domain;
pub mod infrastructure;

use std::sync::Arc;
use application::queries::QueryService;

#[derive(Clone)]
pub struct AppState {
    pub query_svc: Arc<QueryService>,
}
