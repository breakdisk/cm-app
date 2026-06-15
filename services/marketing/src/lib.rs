pub mod api;
pub mod application;
pub mod bootstrap;
pub mod config;
pub mod domain;
pub mod infrastructure;
pub mod mcp;

use std::sync::Arc;
use application::services::CampaignService;
use infrastructure::{db::{PgAbTestRepository, PgJourneyRepository}, external::CdpClient};
use logisticos_auth::jwt::JwtService;

#[derive(Clone)]
pub struct AppState {
    pub campaign_svc:  Arc<CampaignService>,
    pub ab_test_repo:  Arc<PgAbTestRepository>,
    pub journey_repo:  Arc<PgJourneyRepository>,
    pub jwt:           Arc<JwtService>,
    /// Optional CDP client for audience resolution.
    /// Present only when `SERVICES__CDP_URL` and `SERVICES__CDP_TOKEN` are set.
    pub cdp_client:    Option<Arc<CdpClient>>,
}
