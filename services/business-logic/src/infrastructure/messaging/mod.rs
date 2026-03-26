// Kafka producer — business-logic service
// Publishes RuleExecuted events so other services can react
// (e.g., engagement service triggers a campaign after a rule fires).

use rdkafka::producer::{FutureProducer, FutureRecord};
use rdkafka::ClientConfig;
use serde::Serialize;
use std::time::Duration;

pub const TOPIC_RULE_EXECUTED: &str = "rule.executed";

#[derive(Debug, Serialize)]
pub struct RuleExecutedEvent {
    pub rule_id: String,
    pub rule_name: String,
    pub tenant_id: String,
    pub trigger_event_id: String,
    pub actions_fired: Vec<String>,
    pub timestamp: i64,
}

#[derive(Clone)]
pub struct KafkaPublisher {
    producer: FutureProducer,
}

impl KafkaPublisher {
    pub fn new(brokers: &str) -> Result<Self, rdkafka::error::KafkaError> {
        let producer: FutureProducer = ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .set("message.timeout.ms", "5000")
            .create()?;
        Ok(Self { producer })
    }

    pub async fn publish_rule_executed(
        &self,
        event: &RuleExecutedEvent,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let payload = serde_json::to_string(event)?;
        let key = event.rule_id.as_str();
        self.producer
            .send(
                FutureRecord::to(TOPIC_RULE_EXECUTED)
                    .key(key)
                    .payload(&payload),
                Duration::from_secs(5),
            )
            .await
            .map_err(|(e, _)| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
        Ok(())
    }
}
