pub mod api;
pub mod application;
pub mod bootstrap;
pub mod config;
pub mod domain;
pub mod infrastructure;

use std::sync::Arc;
use application::agent::AgentRunner;
use infrastructure::db::SessionRepository;
use infrastructure::tools::ToolRegistry;

#[derive(Clone)]
pub struct AppState {
    pub runner:       Arc<AgentRunner>,
    /// A small, fast model for routing the Move app's prompt box. Separate
    /// from the agent runner's client so a cheap per-send call never runs on
    /// the agents' model.
    pub classifier:   Arc<dyn logisticos_agent_runtime::claude::ClaudeApi>,
    pub session_repo: Arc<dyn SessionRepository>,
    pub tools:        Arc<ToolRegistry>,
    pub jwt:          Arc<logisticos_auth::jwt::JwtService>,
    /// Event producer. `None` when the broker could not be reached at startup —
    /// the service still serves traffic, escalation-resolved notifications just
    /// don't go out (logged, never a request failure).
    pub kafka:        Option<Arc<logisticos_events::producer::KafkaProducer>>,
}
