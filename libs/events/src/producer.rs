//! Kafka producer wrapper using rdkafka.
//! Implements the EventPublisher trait used across all application services.

use rdkafka::{
    config::ClientConfig,
    producer::{FutureProducer, FutureRecord},
    util::Timeout,
};
use serde::Serialize;
use std::time::Duration;
use crate::envelope::Event;

pub struct KafkaProducer {
    inner: FutureProducer,
}

impl KafkaProducer {
    pub fn new(brokers: &str) -> anyhow::Result<Self> {
        let producer: FutureProducer = ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .set("message.timeout.ms", "5000")
            .set("queue.buffering.max.messages", "100000")
            .set("queue.buffering.max.ms", "5")          // low latency
            .set("compression.type", "lz4")
            .set("acks", "1")                             // leader ack — balance of speed vs durability
            .create()?;

        Ok(Self { inner: producer })
    }

    /// Publish a typed domain event to the given topic.
    /// The tenant_id is used as the Kafka partition key to keep
    /// all events for the same tenant on the same partition (order guarantee).
    pub async fn publish_event<T: Serialize>(
        &self,
        topic: &str,
        event: &Event<T>,
    ) -> anyhow::Result<()> {
        let payload = serde_json::to_string(event)?;
        let key = event.tenant_id.to_string();
        self.publish_raw(topic, &key, &payload).await
    }

    /// Publish a `serde_json::Value` payload. Uses an empty string key.
    pub async fn publish_json(&self, topic: &str, payload: &serde_json::Value) -> anyhow::Result<()> {
        let body = serde_json::to_string(payload)?;
        self.publish_raw(topic, "", &body).await
    }

    /// Publish a raw JSON payload. Prefer `publish_event` for typed events.
    pub async fn publish_raw(&self, topic: &str, key: &str, payload: &str) -> anyhow::Result<()> {
        self.inner
            .send(
                FutureRecord::to(topic)
                    .key(key)
                    .payload(payload.as_bytes()),
                Timeout::After(Duration::from_secs(5)),
            )
            .await
            .map_err(|(err, _msg)| anyhow::anyhow!("Kafka publish failed: {err}"))?;
        Ok(())
    }
}
