use std::sync::Arc;
use logisticos_events::{envelope::Event, producer::KafkaProducer};
use uuid::Uuid;
use crate::domain::events::{
    TOPIC_COMPLIANCE, ComplianceStatusChangedPayload,
    DocumentReviewedPayload, ExpiryWarningPayload, DriverReinstatedPayload,
};

pub struct ComplianceProducer {
    kafka: Arc<KafkaProducer>,
}

impl ComplianceProducer {
    pub fn new(kafka: Arc<KafkaProducer>) -> Self { Self { kafka } }

    pub async fn publish_status_changed(
        &self, tenant_id: Uuid, payload: ComplianceStatusChangedPayload,
    ) -> anyhow::Result<()> {
        let event = Event::new("logisticos/compliance", "compliance.status_changed", tenant_id, payload);
        self.kafka.publish_event(TOPIC_COMPLIANCE, &event).await
    }

    pub async fn publish_document_reviewed(
        &self, tenant_id: Uuid, payload: DocumentReviewedPayload,
    ) -> anyhow::Result<()> {
        let event = Event::new("logisticos/compliance", "compliance.document_reviewed", tenant_id, payload);
        self.kafka.publish_event(TOPIC_COMPLIANCE, &event).await
    }

    pub async fn publish_expiry_warning(
        &self, tenant_id: Uuid, payload: ExpiryWarningPayload,
    ) -> anyhow::Result<()> {
        let event = Event::new("logisticos/compliance", "compliance.expiry_warning", tenant_id, payload);
        self.kafka.publish_event(TOPIC_COMPLIANCE, &event).await
    }

    pub async fn publish_driver_reinstated(
        &self, tenant_id: Uuid, payload: DriverReinstatedPayload,
    ) -> anyhow::Result<()> {
        let event = Event::new("logisticos/compliance", "compliance.driver_reinstated", tenant_id, payload);
        self.kafka.publish_event(TOPIC_COMPLIANCE, &event).await
    }
}
