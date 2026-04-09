use async_trait::async_trait;
use logisticos_types::awb::{Awb, ServiceCode, TenantCode};
use sqlx::PgPool;
use tracing::warn;

use crate::domain::value_objects::awb_generator::{AwbGenerator, AwbGeneratorError};

/// Fallback AWB generator using PostgreSQL `nextval`.
///
/// Used when Redis is unavailable.  Each tenant+service pair has a dedicated
/// PostgreSQL sequence created at tenant onboarding.
///
/// Sequence naming: `awb_seq_{tenant}_{service_char}` (e.g. `awb_seq_ph1_s`).
/// All chars lowercased; PostgreSQL identifiers are case-insensitive.
pub struct PostgresAwbGenerator {
    pool: PgPool,
}

impl PostgresAwbGenerator {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    fn sequence_name(tenant: &TenantCode, service: ServiceCode) -> String {
        format!(
            "awb_seq_{}_{}",
            tenant.as_str().to_lowercase(),
            service.as_char().to_lowercase()
        )
    }
}

#[async_trait]
impl AwbGenerator for PostgresAwbGenerator {
    async fn next_awb(
        &self,
        tenant_code: &TenantCode,
        service: ServiceCode,
    ) -> Result<Awb, AwbGeneratorError> {
        warn!(
            tenant = tenant_code.as_str(),
            "Using PostgreSQL AWB generator fallback — Redis may be unavailable"
        );

        let seq_name = Self::sequence_name(tenant_code, service);
        let row: (i64,) = sqlx::query_as(&format!("SELECT nextval('{}')", seq_name))
            .fetch_one(&self.pool)
            .await
            .map_err(|e| AwbGeneratorError::Database(e.to_string()))?;

        let seq = row.0 as u32;
        Ok(Awb::generate(tenant_code, service, seq))
    }
}

/// Create the PostgreSQL sequence for a tenant+service combination.
/// Called once at tenant onboarding.  Idempotent — uses `IF NOT EXISTS`.
pub async fn create_awb_sequence(
    pool: &PgPool,
    tenant_code: &TenantCode,
    service: ServiceCode,
    start_from: u32,
) -> Result<(), AwbGeneratorError> {
    let seq_name = PostgresAwbGenerator::sequence_name(tenant_code, service);
    sqlx::query(&format!(
        "CREATE SEQUENCE IF NOT EXISTS {} START {} MAXVALUE 9999999 NO CYCLE",
        seq_name, start_from
    ))
    .execute(pool)
    .await
    .map_err(|e| AwbGeneratorError::Database(e.to_string()))?;
    Ok(())
}
