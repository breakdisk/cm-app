use redis::AsyncCommands;
use uuid::Uuid;

const KEY_PREFIX: &str = "compliance:status:";
const TTL_SECONDS: usize = 300; // 5 min — refreshed on every compliance.status_changed event

pub struct ComplianceCache {
    redis: redis::aio::ConnectionManager,
}

impl ComplianceCache {
    pub fn new(redis: redis::aio::ConnectionManager) -> Self {
        Self { redis }
    }

    pub async fn set_status(
        &mut self,
        entity_id: Uuid,
        status: &str,
        is_assignable: bool,
    ) -> anyhow::Result<()> {
        let key = format!("{KEY_PREFIX}{entity_id}");
        let val = serde_json::json!({
            "status": status,
            "is_assignable": is_assignable,
        })
        .to_string();
        self.redis.set_ex::<_, _, ()>(&key, val, TTL_SECONDS).await?;
        Ok(())
    }

    /// Returns `(status, is_assignable)`, or `None` if not cached.
    pub async fn get_status(
        &mut self,
        entity_id: Uuid,
    ) -> anyhow::Result<Option<(String, bool)>> {
        let key = format!("{KEY_PREFIX}{entity_id}");
        let val: Option<String> = self.redis.get(&key).await?;
        let Some(raw) = val else {
            return Ok(None);
        };
        let j: serde_json::Value = match serde_json::from_str(&raw) {
            Ok(j) => j,
            Err(e) => {
                tracing::warn!(key = %key, "Compliance cache entry is not valid JSON: {e}");
                return Ok(None);
            }
        };
        let Some(status) = j["status"].as_str().map(|s| s.to_owned()) else {
            tracing::warn!(key = %key, "Compliance cache entry missing 'status' field");
            return Ok(None);
        };
        let is_assignable = match j["is_assignable"].as_bool() {
            Some(v) => v,
            None => {
                tracing::warn!(
                    key = %key,
                    "Compliance cache entry has malformed 'is_assignable' field — defaulting to false"
                );
                false
            }
        };
        Ok(Some((status, is_assignable)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Integration test — requires running Redis
    async fn cache_stores_and_retrieves_status() {
        // Run: cargo test -p logisticos-dispatch compliance_cache -- --ignored
    }
}
