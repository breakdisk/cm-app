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
    pub runner:      Arc<AgentRunner>,
    pub session_repo: Arc<dyn SessionRepository>,
    pub tools:       Arc<ToolRegistry>,
}
