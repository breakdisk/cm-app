use std::sync::Arc;
use logisticos_events::{
    envelope::Event,
    payloads::VehicleRegistered,
    producer::KafkaProducer,
    topics::VEHICLE_REGISTERED,
};
use uuid::Uuid;

pub struct FleetPublisher {
    kafka: Arc<KafkaProducer>,
}

impl FleetPublisher {
    pub fn new(kafka: Arc<KafkaProducer>) -> Self { Self { kafka } }

    pub async fn vehicle_registered(
        &self,
        vehicle_id:    Uuid,
        tenant_id:     Uuid,
        vehicle_class: String,
        jurisdiction:  String,
    ) -> anyhow::Result<()> {
        let payload = VehicleRegistered { vehicle_id, tenant_id, jurisdiction, vehicle_class };
        let event   = Event::new("logisticos/fleet", "vehicle.registered", tenant_id, payload);
        self.kafka.publish_event(VEHICLE_REGISTERED, &event).await
    }
}
