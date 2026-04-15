// Redis cache adapter — business-logic service
// Stores hot rule evaluation results (e.g., per-tenant enabled rule IDs)
// to avoid repeated DB queries on every Kafka event.

use redis::aio::MultiplexedConnection;
use redis::AsyncCommands;

const RULE_TTL: u64 = 300; // 5 minutes

pub struct RuleCache {
    conn: MultiplexedConnection,
}

impl RuleCache {
    pub fn new(conn: MultiplexedConnection) -> Self {
        Self { conn }
    }

    /// Cache the set of active rule IDs for a tenant.
    pub async fn set_active_rule_ids(
        &mut self,
        tenant_id: &str,
        rule_ids: &[String],
    ) -> Result<(), redis::RedisError> {
        let key = format!("biz:rules:active:{tenant_id}");
        let value = rule_ids.join(",");
        self.conn
            .set_ex(key, value, RULE_TTL)
            .await
    }

    /// Returns None if cache miss or expired.
    pub async fn get_active_rule_ids(
        &mut self,
        tenant_id: &str,
    ) -> Result<Option<Vec<String>>, redis::RedisError> {
        let key = format!("biz:rules:active:{tenant_id}");
        let raw: Option<String> = self.conn.get(key).await?;
        Ok(raw.map(|s| {
            s.split(',')
                .filter(|id| !id.is_empty())
                .map(String::from)
                .collect()
        }))
    }

    /// Invalidate the tenant's rule cache (call after any rule mutation).
    pub async fn invalidate_tenant(
        &mut self,
        tenant_id: &str,
    ) -> Result<(), redis::RedisError> {
        let key = format!("biz:rules:active:{tenant_id}");
        self.conn.del(key).await
    }
}
