pub mod status_consumer;

use std::pin::Pin;
use std::future::Future;

use rdkafka::config::ClientConfig;
use rdkafka::producer::{FutureProducer, FutureRecord};
use std::time::Duration;

use crate::application::services::shipment_service::EventPublisher;

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
