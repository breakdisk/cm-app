use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use redis::AsyncCommands;
use uuid::Uuid;

use logisticos_types::TenantId;

use crate::domain::{
    entities::{Carrier, CarrierId},
    repositories::CarrierRepository,
};

// ── TTLs ──────────────────────────────────────────────────────────────────────

const CARRIER_BY_ID_TTL:    Duration = Duration::from_secs(300);  // 5 min
const CARRIER_BY_EMAIL_TTL: Duration = Duration::from_secs(300);

fn key_by_id(id: Uuid) -> String {
    format!("carrier:id:{id}")
}

fn key_by_email(tenant_id: Uuid, email: &str) -> String {
    format!("carrier:email:{tenant_id}:{}", email.to_lowercase())
}

fn key_active_list(tenant_id: Uuid) -> String {
    format!("carrier:active_list:{tenant_id}")
}

// ── Cached repository ─────────────────────────────────────────────────────────

/// Write-through cache layer over `PgCarrierRepository`.
///
/// Read path: check Redis → on miss, load from Postgres and backfill cache.
/// Write path: persist to Postgres first, then invalidate affected keys so the
/// next read pulls fresh data (cache-aside invalidation is safer than
/// write-through for mutable aggregates).
///
/// Carrier data changes infrequently (admin actions only) so a 5-minute TTL
/// keeps memory usage low while eliminating repetitive `/me` lookups on every
/// partner-portal page load.
pub struct CachedCarrierRepository {
    db:    Arc<dyn CarrierRepository>,
    redis: redis::aio::MultiplexedConnection,
}

impl CachedCarrierRepository {
    pub async fn new(
        db:    Arc<dyn CarrierRepository>,
        redis_url: &str,
    ) -> anyhow::Result<Self> {
        let client = redis::Client::open(redis_url)?;
        let redis  = client.get_multiplexed_async_connection().await?;
        Ok(Self { db, redis })
    }

    async fn get_cached<T: serde::de::DeserializeOwned>(&self, key: &str) -> Option<T> {
        let mut conn = self.redis.clone();
        let raw: Option<String> = conn.get(key).await.ok()?;
        raw.as_deref().and_then(|s| serde_json::from_str(s).ok())
    }

    async fn set_cached<T: serde::Serialize>(&self, key: &str, value: &T, ttl: Duration) {
        let Ok(json) = serde_json::to_string(value) else { return };
        let mut conn = self.redis.clone();
        let _: Result<(), _> = conn
            .set_ex(key, json, ttl.as_secs())
            .await;
    }

    async fn del_cached(&self, keys: &[String]) {
        if keys.is_empty() { return; }
        let mut conn = self.redis.clone();
        let _: Result<(), _> = conn.del(keys).await;
    }
}

#[async_trait]
impl CarrierRepository for CachedCarrierRepository {
    async fn find_by_id(&self, id: &CarrierId) -> anyhow::Result<Option<Carrier>> {
        let k = key_by_id(id.inner());
        if let Some(cached) = self.get_cached::<Carrier>(&k).await {
            return Ok(Some(cached));
        }
        let result = self.db.find_by_id(id).await?;
        if let Some(ref carrier) = result {
            self.set_cached(&k, carrier, CARRIER_BY_ID_TTL).await;
        }
        Ok(result)
    }

    async fn find_by_code(&self, tenant_id: &TenantId, code: &str) -> anyhow::Result<Option<Carrier>> {
        // Code lookups are rare (onboarding uniqueness check only) — skip cache.
        self.db.find_by_code(tenant_id, code).await
    }

    async fn find_by_contact_email(&self, tenant_id: &TenantId, email: &str) -> anyhow::Result<Option<Carrier>> {
        let k = key_by_email(tenant_id.inner(), email);
        if let Some(cached) = self.get_cached::<Carrier>(&k).await {
            return Ok(Some(cached));
        }
        let result = self.db.find_by_contact_email(tenant_id, email).await?;
        if let Some(ref carrier) = result {
            self.set_cached(&k, carrier, CARRIER_BY_EMAIL_TTL).await;
            // Also warm the by-id cache from the same fetch.
            let id_key = key_by_id(carrier.id.inner());
            self.set_cached(&id_key, carrier, CARRIER_BY_ID_TTL).await;
        }
        Ok(result)
    }

    async fn list(&self, tenant_id: &TenantId, limit: i64, offset: i64) -> anyhow::Result<Vec<Carrier>> {
        // List results are paginated and may vary by parameters — always
        // read from Postgres to avoid stale counts on admin pages.
        self.db.list(tenant_id, limit, offset).await
    }

    async fn list_active(&self, tenant_id: &TenantId) -> anyhow::Result<Vec<Carrier>> {
        let k = key_active_list(tenant_id.inner());
        if let Some(cached) = self.get_cached::<Vec<Carrier>>(&k).await {
            return Ok(cached);
        }
        let result = self.db.list_active(tenant_id).await?;
        // Short TTL — rate-shop uses this and needs near-real-time carrier availability.
        self.set_cached(&k, &result, Duration::from_secs(60)).await;
        Ok(result)
    }

    async fn save(&self, carrier: &Carrier) -> anyhow::Result<()> {
        // Persist first; invalidate second (never cache a write that hasn't landed).
        self.db.save(carrier).await?;
        let keys = vec![
            key_by_id(carrier.id.inner()),
            key_by_email(carrier.tenant_id.inner(), &carrier.contact_email),
            key_active_list(carrier.tenant_id.inner()),
        ];
        self.del_cached(&keys).await;
        Ok(())
    }
}
