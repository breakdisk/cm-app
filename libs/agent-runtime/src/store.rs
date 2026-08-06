//! Persistence contract for crash recovery.
//!
//! Deliberately narrow: the runner saves after every turn and reloads by id.
//! Listing, filtering and aggregation are product dashboard concerns and stay
//! with the product.

use async_trait::async_trait;
use uuid::Uuid;

use crate::session::AgentSession;

#[async_trait]
pub trait SessionStore: Send + Sync {
    async fn save(&self, session: &AgentSession) -> anyhow::Result<()>;
    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<AgentSession>>;
}
