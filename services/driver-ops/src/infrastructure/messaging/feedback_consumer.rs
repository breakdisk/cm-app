//! Consumes `delivery.feedback.submitted` (delivery-experience) and folds the
//! customer's rating into `driver_ops.drivers.rating_avg` / `rating_count`.
//!
//! Id mapping: the event's `driver_id` is the id dispatch assigned the shipment
//! to, which is the identity user id and — by the `drivers_id_is_user_id` check
//! from migration 0014 — equal to `driver_ops.drivers.id`. We match on `id`
//! AND the envelope's `tenant_id`; a driver in another tenant is never touched.
//!
//! Delivery is at-least-once, so the dedupe row (`driver_rating_events`, keyed
//! by tenant + shipment) and the aggregate update commit in ONE transaction.

use logisticos_events::{envelope::Event, payloads::DeliveryFeedbackSubmitted, topics};
use rdkafka::{
    consumer::{CommitMode, Consumer, StreamConsumer},
    config::ClientConfig,
    Message,
};
use sqlx::PgPool;
use tokio::sync::watch;
use uuid::Uuid;

pub async fn start_feedback_consumer(
    brokers: &str,
    group_id: &str,
    pool: PgPool,
    mut shutdown: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", brokers)
        .set("group.id", format!("{}-driver-feedback", group_id))
        .set("auto.offset.reset", "earliest")
        .set("enable.auto.commit", "false")
        .create()?;

    consumer.subscribe(&[topics::DELIVERY_FEEDBACK_SUBMITTED])?;
    tracing::info!("feedback consumer: subscribed to {}", topics::DELIVERY_FEEDBACK_SUBMITTED);

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    tracing::info!("feedback consumer: shutdown signal received");
                    break;
                }
            }
            result = consumer.recv() => {
                match result {
                    Ok(msg) => {
                        if let Some(payload) = msg.payload() {
                            // Retried in place; the handler is replay-safe.
                            let mut attempt = 0u32;
                            while let Err(e) = handle_feedback(payload, &pool).await {
                                attempt += 1;
                                if attempt >= 6 {
                                    tracing::error!(offset = msg.offset(), err = %e,
                                        "feedback consumer: failed 6 times — rating not applied");
                                    break;
                                }
                                tracing::warn!(attempt, err = %e, "feedback consumer: handler error — retrying");
                                tokio::time::sleep(std::time::Duration::from_secs(2u64.pow(attempt.min(5)))).await;
                            }
                        }
                        consumer.commit_message(&msg, CommitMode::Async).ok();
                    }
                    Err(e) => {
                        tracing::error!(err = %e, "feedback consumer: recv error");
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    }
                }
            }
        }
    }
    Ok(())
}

/// A rating worth applying, read off the envelope.
#[derive(Debug, PartialEq)]
struct Rating {
    tenant_id:   Uuid,
    event_id:    Uuid,
    shipment_id: Uuid,
    driver_id:   Uuid,
    rating:      i16,
}

/// Why an event was skipped without touching the database.
#[derive(Debug, PartialEq)]
enum Skip {
    NoTenant,
    RatingOutOfRange(i16),
}

fn parse(payload: &[u8]) -> anyhow::Result<Result<Rating, Skip>> {
    let event: Event<DeliveryFeedbackSubmitted> = serde_json::from_slice(payload)?;
    if event.tenant_id.is_nil() {
        return Ok(Err(Skip::NoTenant));
    }
    if !(1..=5).contains(&event.data.rating) {
        return Ok(Err(Skip::RatingOutOfRange(event.data.rating)));
    }
    Ok(Ok(Rating {
        tenant_id:   event.tenant_id,
        event_id:    event.id,
        shipment_id: event.data.shipment_id,
        driver_id:   event.data.driver_id,
        rating:      event.data.rating,
    }))
}

#[derive(Debug, PartialEq, Eq)]
pub enum RatingOutcome {
    Applied,
    /// This shipment's rating was already counted.
    Duplicate,
    /// No such driver in this tenant.
    UnknownDriver,
}

/// `(avg * count + r) / (count + 1)`, with a never-rated driver as 0 / 0.
fn next_average(avg: Option<f32>, count: i32, rating: i16) -> f32 {
    let total = f64::from(avg.unwrap_or(0.0)) * f64::from(count) + f64::from(rating);
    (total / f64::from(count + 1)) as f32
}

async fn handle_feedback(payload: &[u8], pool: &PgPool) -> anyhow::Result<()> {
    let r = match parse(payload)? {
        Ok(r) => r,
        Err(skip) => {
            tracing::warn!(?skip, "feedback consumer: event skipped");
            return Ok(());
        }
    };
    match apply_rating(pool, r.tenant_id, r.event_id, r.shipment_id, r.driver_id, r.rating).await? {
        RatingOutcome::Applied => tracing::info!(
            driver_id = %r.driver_id, shipment_id = %r.shipment_id, rating = r.rating,
            "feedback consumer: driver rating updated"
        ),
        RatingOutcome::Duplicate => tracing::info!(
            shipment_id = %r.shipment_id, "feedback consumer: rating already counted — skipped"
        ),
        RatingOutcome::UnknownDriver => tracing::warn!(
            driver_id = %r.driver_id, shipment_id = %r.shipment_id, tenant_id = %r.tenant_id,
            "feedback consumer: no such driver in this tenant — rating skipped"
        ),
    }
    Ok(())
}

/// Count one rating against a driver, exactly once per (tenant, shipment).
///
/// One transaction: lock the driver row (tenant-scoped), claim the dedupe row,
/// write the new aggregate. Any early return drops the transaction, so an
/// unknown driver or a duplicate leaves no trace.
pub async fn apply_rating(
    pool: &PgPool,
    tenant_id: Uuid,
    event_id: Uuid,
    shipment_id: Uuid,
    driver_id: Uuid,
    rating: i16,
) -> anyhow::Result<RatingOutcome> {
    let mut tx = pool.begin().await?;

    let current: Option<(Option<f32>, i32)> = sqlx::query_as(
        "SELECT rating_avg, rating_count FROM driver_ops.drivers
          WHERE id = $1 AND tenant_id = $2 FOR UPDATE",
    )
    .bind(driver_id)
    .bind(tenant_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((avg, count)) = current else {
        return Ok(RatingOutcome::UnknownDriver);
    };

    let claimed = sqlx::query(
        "INSERT INTO driver_ops.driver_rating_events (tenant_id, shipment_id, event_id, driver_id, rating)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (tenant_id, shipment_id) DO NOTHING",
    )
    .bind(tenant_id)
    .bind(shipment_id)
    .bind(event_id)
    .bind(driver_id)
    .bind(rating)
    .execute(&mut *tx)
    .await?;
    if claimed.rows_affected() == 0 {
        return Ok(RatingOutcome::Duplicate);
    }

    sqlx::query(
        "UPDATE driver_ops.drivers
            SET rating_avg = $3, rating_count = $4, updated_at = NOW()
          WHERE id = $1 AND tenant_id = $2",
    )
    .bind(driver_id)
    .bind(tenant_id)
    .bind(next_average(avg, count, rating))
    .bind(count + 1)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(RatingOutcome::Applied)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn envelope(tenant: Uuid, rating: i16) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "id": Uuid::new_v4(),
            "source": "logisticos/delivery-experience",
            "event_type": "delivery.feedback.submitted",
            "time": "2026-11-28T08:00:00Z",
            "tenant_id": tenant,
            "data": { "shipment_id": Uuid::new_v4(), "driver_id": Uuid::new_v4(), "rating": rating },
        }))
        .expect("serializes")
    }

    #[test]
    fn first_rating_becomes_the_average() {
        assert_eq!(next_average(None, 0, 4), 4.0);
    }

    #[test]
    fn rating_updates_the_running_average() {
        // (4.0 * 3 + 5) / 4
        assert!((next_average(Some(4.0), 3, 5) - 4.25).abs() < 1e-6);
        // (5.0 * 1 + 1) / 2
        assert!((next_average(Some(5.0), 1, 1) - 3.0).abs() < 1e-6);
    }

    #[test]
    fn reads_the_published_event() {
        let tenant = Uuid::new_v4();
        let parsed = parse(&envelope(tenant, 4)).expect("parses").expect("not skipped");
        assert_eq!(parsed.tenant_id, tenant);
        assert_eq!(parsed.rating, 4);
    }

    #[test]
    fn skips_nil_tenant_and_out_of_range_ratings() {
        assert_eq!(parse(&envelope(Uuid::nil(), 4)).expect("parses"), Err(Skip::NoTenant));
        assert_eq!(parse(&envelope(Uuid::new_v4(), 0)).expect("parses"), Err(Skip::RatingOutOfRange(0)));
        assert_eq!(parse(&envelope(Uuid::new_v4(), 6)).expect("parses"), Err(Skip::RatingOutOfRange(6)));
    }

    #[test]
    fn malformed_payload_is_an_error_not_a_skip() {
        assert!(parse(b"{}").is_err());
    }
}
