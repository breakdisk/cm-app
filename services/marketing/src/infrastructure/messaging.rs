//! Kafka-backed implementation of `EventPublisher` for the marketing service.
//!
//! `KafkaEventPublisher` wraps an `rdkafka::FutureProducer` and exposes the
//! `EventPublisher` trait used by `CampaignService`.  Constructed in bootstrap.rs
//! and passed into the service as a trait object so the service remains testable.

use std::time::Duration;
use rdkafka::producer::{FutureProducer, FutureRecord};

use crate::application::services::EventPublisher;

pub struct KafkaEventPublisher {
    producer: FutureProducer,
}

impl KafkaEventPublisher {
    pub fn new(producer: FutureProducer) -> Self {
        Self { producer }
    }
}

#[async_trait::async_trait]
impl EventPublisher for KafkaEventPublisher {
    async fn publish(&self, topic: &str, key: &str, payload: &[u8]) -> anyhow::Result<()> {
        self.producer
            .send(
                FutureRecord::to(topic).key(key).payload(payload),
                Duration::from_secs(5),
            )
            .await
            .map_err(|(e, _)| anyhow::anyhow!("Kafka publish error on topic {topic}: {e}"))?;
        Ok(())
    }
}
