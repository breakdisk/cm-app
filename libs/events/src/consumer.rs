//! Kafka consumer helper.
//! Each service creates a ConsumerGroup pointing at its subscribed topics.

use rdkafka::{
    config::ClientConfig,
    consumer::{CommitMode, Consumer, StreamConsumer},
    Message,
};
use tracing::{error, info, warn};

pub struct KafkaConsumer {
    inner: StreamConsumer,
}

impl KafkaConsumer {
    pub fn new(brokers: &str, group_id: &str, topics: &[&str]) -> anyhow::Result<Self> {
        let consumer: StreamConsumer = ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .set("group.id", group_id)
            .set("enable.auto.commit", "false")   // manual commit for at-least-once delivery
            .set("auto.offset.reset", "earliest")
            .set("max.poll.interval.ms", "300000")
            .create()?;

        consumer.subscribe(topics)?;
        info!(group_id, ?topics, "Kafka consumer subscribed");

        Ok(Self { inner: consumer })
    }

    /// Process messages in a loop, calling `handler` for each.
    /// Commits offset only after the handler returns Ok(()).
    /// On error: logs and skips (dead-letter behaviour can be added here).
    pub async fn run<F, Fut>(&self, mut handler: F) -> anyhow::Result<()>
    where
        F: FnMut(String, serde_json::Value) -> Fut,
        Fut: std::future::Future<Output = anyhow::Result<()>>,
    {
        loop {
            match self.inner.recv().await {
                Err(e) => error!("Kafka receive error: {e}"),
                Ok(msg) => {
                    let topic = msg.topic().to_owned();
                    let payload = match msg.payload_view::<str>() {
                        Some(Ok(s)) => s.to_owned(),
                        Some(Err(e)) => { warn!("Non-UTF8 Kafka payload: {e}"); continue; }
                        None        => { warn!("Empty Kafka payload"); continue; }
                    };

                    let json: serde_json::Value = match serde_json::from_str(&payload) {
                        Ok(v) => v,
                        Err(e) => { error!("Kafka JSON parse error: {e}"); continue; }
                    };

                    match handler(topic.clone(), json).await {
                        Ok(()) => {
                            // Safe to commit — message fully processed
                            if let Err(e) = self.inner.commit_message(&msg, CommitMode::Async) {
                                warn!("Kafka commit failed: {e}");
                            }
                        }
                        Err(e) => {
                            error!(topic, "Handler error (message not committed): {e}");
                            // Message will be redelivered — implement dead-letter for poison pills
                        }
                    }
                }
            }
        }
    }
}
