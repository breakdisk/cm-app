//! Kafka event publisher for delivery-experience.
//!
//! Currently used only for `receipt.email.requested` — fired when a customer
//! taps "Email Receipt" on the tracking/receipt screen. Engagement consumes it
//! and dispatches the email via the existing notification pipeline.

use std::pin::Pin;
use std::future::Future;
use std::time::Duration;

use rdkafka::config::ClientConfig;
use rdkafka::producer::{FutureProducer, FutureRecord};

use crate::application::services::EventPublisher;

pub struct KafkaEventPublisher {
    producer: FutureProducer,
}

impl KafkaEventPublisher {
    pub fn new(brokers: &str) -> anyhow::Result<Self> {
        let producer: FutureProducer = ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .set("message.timeout.ms", "5000")
            .set("acks", "all")
            .create()?;
        Ok(Self { producer })
    }
}

impl EventPublisher for KafkaEventPublisher {
    fn publish<'a>(
        &'a self,
        topic: &'a str,
        key: &'a str,
        payload: &'a str,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + 'a>> {
        Box::pin(async move {
            self.producer
                .send(
                    FutureRecord::to(topic).key(key).payload(payload),
                    Duration::from_secs(5),
                )
                .await
                .map_err(|(e, _)| anyhow::anyhow!("Kafka publish error: {e}"))?;
            Ok(())
        })
    }
}
