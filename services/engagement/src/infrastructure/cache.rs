//! Campaign suppression cache — Redis-backed per-customer suppression flags.
//!
//! When a customer has an open support ticket, all marketing/campaign
//! notifications are suppressed until the ticket is resolved. This avoids
//! sending promotional messages while the customer is actively unhappy.
//!
//! Key schema: `engagement:suppress:{customer_id}` (string, TTL-backed).
//! Written by the support ticket event handler; read in the notification path.

use redis::AsyncCommands;
use uuid::Uuid;

/// TTL for a suppression flag. Long enough to cover P95 support resolution time.
/// If a `ticket.closed` event is never received the flag self-expires after 72h.
const SUPPRESSION_TTL_SECS: u64 = 72 * 3600;

pub struct SuppressionCache {
    conn: redis::aio::ConnectionManager,
}

impl SuppressionCache {
    pub async fn new(redis_url: &str) -> anyhow::Result<Self> {
        let client = redis::Client::open(redis_url)?;
        let conn = redis::aio::ConnectionManager::new(client).await?;
        Ok(Self { conn })
    }

    fn key(customer_id: Uuid) -> String {
        format!("engagement:suppress:{}", customer_id)
    }

    /// Mark a customer as suppressed (support ticket opened).
    pub async fn suppress(&self, customer_id: Uuid) -> anyhow::Result<()> {
        let mut conn = self.conn.clone();
        conn.set_ex::<_, _, ()>(Self::key(customer_id), "1", SUPPRESSION_TTL_SECS).await?;
        Ok(())
    }

    /// Remove suppression flag (support ticket closed).
    pub async fn lift(&self, customer_id: Uuid) -> anyhow::Result<()> {
        let mut conn = self.conn.clone();
        conn.del::<_, ()>(Self::key(customer_id)).await?;
        Ok(())
    }

    /// Returns true if marketing messages should be suppressed for this customer.
    pub async fn is_suppressed(&self, customer_id: Uuid) -> bool {
        let mut conn = self.conn.clone();
        conn.exists::<_, bool>(Self::key(customer_id)).await.unwrap_or(false)
    }
}
