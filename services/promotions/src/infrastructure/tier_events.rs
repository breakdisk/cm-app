//! Tier upgrades, announced on Kafka for engagement to push to the customer.

use async_trait::async_trait;
use logisticos_events::{envelope::Event, payloads::TierChanged, topics, KafkaProducer};

use crate::application::TierEvents;

pub struct KafkaTierEvents {
    producer: KafkaProducer,
}

impl KafkaTierEvents {
    pub fn new(producer: KafkaProducer) -> Self {
        Self { producer }
    }
}

#[async_trait]
impl TierEvents for KafkaTierEvents {
    async fn tier_changed(&self, e: &TierChanged) -> anyhow::Result<()> {
        let event = Event::new("logisticos/promotions", "promotions.tier.changed", e.tenant_id, e.clone());
        self.producer.publish_event(topics::TIER_CHANGED, &event).await
    }
}
