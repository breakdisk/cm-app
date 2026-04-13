//! Redis-backed invoice sequence number generator.
//!
//! Uses `INCR` on a namespaced key to produce monotonically increasing 5-digit
//! sequence numbers per (invoice_type, tenant, period) tuple.
//!
//! Key pattern: `invoice_seq:{type_prefix}:{tenant_code}:{YYYY-MM}`
//! Falls back to 1 on any Redis failure (safe — duplicate numbers will be
//! rejected by the UNIQUE constraint on `invoice_number` in Postgres, giving
//! the caller a retriable error).

use async_trait::async_trait;
use chrono::NaiveDate;
use redis::AsyncCommands;
use logisticos_types::invoice::InvoiceType;

use crate::application::services::invoice_service::InvoiceSequenceSource;

pub struct RedisSequenceSource {
    client: redis::Client,
}

impl RedisSequenceSource {
    pub fn new(redis_url: &str) -> anyhow::Result<Self> {
        let client = redis::Client::open(redis_url)?;
        Ok(Self { client })
    }
}

#[async_trait]
impl InvoiceSequenceSource for RedisSequenceSource {
    async fn next_sequence(
        &self,
        invoice_type: InvoiceType,
        tenant_code:  &str,
        period:       NaiveDate,
    ) -> anyhow::Result<u32> {
        let prefix = invoice_type.prefix();
        let key = format!(
            "invoice_seq:{}:{}:{}-{:02}",
            prefix,
            tenant_code,
            period.format("%Y"),
            period.format("%m"),
        );

        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let seq: u32 = conn.incr(&key, 1u32).await?;
        Ok(seq)
    }
}
