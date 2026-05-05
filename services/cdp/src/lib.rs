pub mod api;
pub mod application;
pub mod bootstrap;
pub mod config;
pub mod domain;
pub mod infrastructure;

use std::sync::Arc;
use application::services::ProfileService;
use logisticos_auth::jwt::JwtService;
use logisticos_events::producer::KafkaProducer;

#[derive(Clone)]
pub struct AppState {
    pub profile_svc: Arc<ProfileService>,
    pub jwt:         Arc<JwtService>,
    pub kafka:       Arc<KafkaProducer>,
}
