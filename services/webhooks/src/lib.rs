pub mod api;
pub mod application;
pub mod bootstrap;
pub mod config;
pub mod domain;
pub mod infrastructure;

use std::sync::Arc;
use logisticos_auth::jwt::JwtService;
use application::services::WebhookService;

#[derive(Clone)]
pub struct AppState {
    pub webhook_svc: Arc<WebhookService>,
    pub jwt:         Arc<JwtService>,
}
