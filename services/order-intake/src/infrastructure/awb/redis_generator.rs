use async_trait::async_trait;
use logisticos_types::awb::{Awb, ServiceCode, TenantCode};
use redis::AsyncCommands;
use tracing::{error, warn};

use crate::domain::value_objects::awb_generator::{AwbGenerator, AwbGeneratorError};

/// Maximum sequence before the tenant code needs a suffix bump (PH1 → PH2).
const MAX_SEQUENCE: u32 = 9_999_999;

/// Redis key pattern: `awb:seq:{tenant}:{service_char}`
fn seq_key(tenant: &TenantCode, service: ServiceCode) -> String {
    format!("awb:seq:{}:{}", tenant.as_str(), service.as_char())
}

/// Primary AWB generator — uses Redis INCR for atomic, high-throughput sequence allocation.
///
/// A single `INCR` command is atomic in Redis — no transaction needed.
/// TTL is not set on the key so sequences persist across Redis restarts;
/// the key is only initialised on first use via `SET ... NX` in the caller
/// (tenant onboarding sets the starting value).
pub struct RedisAwbGenerator {
    pool: redis::aio::ConnectionManager,
}

impl RedisAwbGenerator {
    pub fn new(pool: redis::aio::ConnectionManager) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AwbGenerator for RedisAwbGenerator {
    async fn next_awb(
        &self,
        tenant_code: &TenantCode,
        service: ServiceCode,
    ) -> Result<Awb, AwbGeneratorError> {
        let key = seq_key(tenant_code, service);
        let mut conn = self.pool.clone();

        let seq: u64 = conn
            .incr(&key, 1u64)
            .await
            .map_err(|e| AwbGeneratorError::Redis(e.to_string()))?;

        if seq > MAX_SEQUENCE as u64 {
            error!(
                tenant = tenant_code.as_str(),
                service = service.as_char() as u32,
                seq,
                "AWB sequence overflow"
            );
            return Err(AwbGeneratorError::SequenceOverflow {
                tenant: tenant_code.as_str().to_string(),
                service: service.as_char(),
            });
        }

        Ok(Awb::generate(tenant_code, service, seq as u32))
    }
}

/// Seed the Redis counter for a new tenant+service combination.
/// Called once during tenant onboarding or service activation.
/// Uses SET NX so it never overwrites an existing counter.
pub async fn seed_awb_counter(
    conn: &mut impl AsyncCommands,
    tenant_code: &TenantCode,
    service: ServiceCode,
    start_from: u32,
) -> Result<bool, AwbGeneratorError> {
    let key = seq_key(tenant_code, service);
    // SET key (start-1) NX — INCR will bring it to `start_from` on first call
    let seeded: bool = redis::cmd("SET")
        .arg(&key)
        .arg(start_from.saturating_sub(1))
        .arg("NX")
        .query_async(conn)
        .await
        .map_err(|e| AwbGeneratorError::Redis(e.to_string()))?;

    if !seeded {
        warn!(
            tenant = tenant_code.as_str(),
            service = service.as_char() as u32,
            "AWB counter already seeded — seed call ignored"
        );
    }

    Ok(seeded)
}
