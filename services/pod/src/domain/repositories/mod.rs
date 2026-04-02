use async_trait::async_trait;
use uuid::Uuid;
use crate::domain::entities::{ProofOfDelivery, OtpCode};

#[async_trait]
pub trait PodRepository: Send + Sync {
    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<ProofOfDelivery>>;
    async fn find_by_shipment(&self, shipment_id: Uuid) -> anyhow::Result<Option<ProofOfDelivery>>;
    async fn save(&self, pod: &ProofOfDelivery) -> anyhow::Result<()>;
}

#[async_trait]
pub trait OtpRepository: Send + Sync {
    async fn find_active_by_shipment(&self, shipment_id: Uuid, tenant_id: Uuid) -> anyhow::Result<Option<OtpCode>>;
    async fn save(&self, otp: &OtpCode) -> anyhow::Result<()>;
}
