pub mod api;
pub mod application;
pub mod bootstrap;
pub mod config;
pub mod domain;
pub mod infrastructure;

use std::sync::Arc;
use application::services::{ConsentService, ProfileService, SegmentService};
use logisticos_auth::jwt::JwtService;
use logisticos_events::producer::KafkaProducer;

#[derive(Clone)]
pub struct AppState {
    pub profile_svc:  Arc<ProfileService>,
    pub segment_svc:  Arc<SegmentService>,
    pub consent_svc:  Arc<ConsentService>,
    pub jwt:          Arc<JwtService>,
    pub kafka:        Arc<KafkaProducer>,
}
