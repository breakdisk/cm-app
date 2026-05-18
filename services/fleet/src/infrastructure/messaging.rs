use logisticos_events::producer::KafkaProducer;
use crate::domain::events::VehicleEvent;

pub struct FleetEventPublisher {
    kafka: logisticos_events::producer::KafkaProducer,
}

impl FleetEventPublisher {
    pub fn new(kafka: KafkaProducer) -> Self {
        Self { kafka }
    }

    pub async fn publish_vehicle_event(&self, event: VehicleEvent) -> anyhow::Result<()> {
        let topic = "fleet.vehicles";
        let _partition = self.kafka.send_json(topic, &event).await?;
        Ok(())
    }
}
