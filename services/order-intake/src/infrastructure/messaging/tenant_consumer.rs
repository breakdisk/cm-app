//! Kafka consumer that seeds AWB counters for new tenants.
//!
//! When the identity service publishes a `TenantCreated` event, this consumer:
//! 1. Derives the 3-char tenant code from the tenant slug.
//! 2. Creates PostgreSQL sequences (idempotent: IF NOT EXISTS) for all 5 service codes.
//! 3. Seeds Redis counters (idempotent: SET NX) for all 5 service codes.
//!
//! Without this seeding, the Postgres fallback AWB generator panics with
//! "relation does not exist" on a new tenant's first shipment when Redis is unavailable.

use logisticos_events::{payloads::TenantCreated, topics, Event};
use logisticos_types::awb::{ServiceCode, TenantCode};
use rdkafka::{
    consumer::{CommitMode, Consumer, StreamConsumer},
    Message,
};
use sqlx::PgPool;

use crate::infrastructure::awb::{
    postgres_generator::create_awb_sequence, redis_generator::seed_awb_counter,
};

const ALL_SERVICE_CODES: [ServiceCode; 5] = [
    ServiceCode::Standard,
    ServiceCode::Express,
    ServiceCode::SameDay,
    ServiceCode::Balikbayan,
    ServiceCode::International,
];

pub async fn start_tenant_consumer(
    brokers: &str,
    group_id: &str,
    pool: PgPool,
    redis: redis::aio::ConnectionManager,
) -> anyhow::Result<()> {
    use rdkafka::config::ClientConfig;
    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", brokers)
        .set("group.id", &format!("{}-awb-seed", group_id))
        .set("auto.offset.reset", "earliest")
        .set("enable.auto.commit", "false")
        .create()?;

    consumer.subscribe(&[topics::TENANT_CREATED])?;

    loop {
        match consumer.recv().await {
            Ok(msg) => {
                let payload = match msg.payload() {
                    Some(p) => p,
                    None => {
                        consumer.commit_message(&msg, CommitMode::Async).ok();
                        continue;
                    }
                };
                if let Err(e) = handle_tenant_created(&pool, &redis, payload).await {
                    tracing::warn!(err = %e, "tenant consumer: handler error (skipping)");
                }
                consumer.commit_message(&msg, CommitMode::Async).ok();
            }
            Err(e) => {
                tracing::error!(err = %e, "tenant consumer: recv error");
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        }
    }
}

async fn handle_tenant_created(
    pool: &PgPool,
    redis: &redis::aio::ConnectionManager,
    payload: &[u8],
) -> anyhow::Result<()> {
    let envelope: Event<TenantCreated> = serde_json::from_slice(payload)?;
    let data = envelope.data;

    // Derive 3-char tenant code from slug: first 3 chars that are ASCII alphanumeric
    // and neither 'O' nor 'I', uppercased.
    let code_str: String = data
        .slug
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() && *c != 'O' && *c != 'o' && *c != 'I' && *c != 'i')
        .take(3)
        .map(|c| c.to_ascii_uppercase())
        .collect();

    if code_str.len() < 3 {
        tracing::error!(
            slug = %data.slug,
            derived = %code_str,
            "tenant consumer: could not derive 3-char tenant code from slug — skipping"
        );
        return Ok(());
    }

    let tenant_code = match TenantCode::new(&code_str) {
        Ok(tc) => tc,
        Err(e) => {
            tracing::error!(
                slug = %data.slug,
                code = %code_str,
                err = %e,
                "tenant consumer: derived tenant code is invalid — skipping"
            );
            return Ok(());
        }
    };

    for service in ALL_SERVICE_CODES {
        // Postgres sequence — idempotent (IF NOT EXISTS)
        if let Err(e) = create_awb_sequence(pool, &tenant_code, service, 1).await {
            tracing::warn!(
                tenant = %data.slug,
                code = %code_str,
                service = service.as_char() as u32,
                err = %e,
                "tenant consumer: failed to create Postgres AWB sequence"
            );
        }

        // Redis counter — idempotent (SET NX)
        let mut redis_conn = redis.clone();
        if let Err(e) = seed_awb_counter(&mut redis_conn, &tenant_code, service, 1).await {
            tracing::warn!(
                tenant = %data.slug,
                code = %code_str,
                service = service.as_char() as u32,
                err = %e,
                "tenant consumer: failed to seed Redis AWB counter"
            );
        }
    }

    tracing::info!(
        tenant = %data.slug,
        code = %code_str,
        services = 5,
        "tenant consumer: AWB counters seeded"
    );

    Ok(())
}
