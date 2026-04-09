use async_trait::async_trait;
use logisticos_types::awb::{Awb, ServiceCode, TenantCode};
use std::sync::Arc;
use tracing::error;

use crate::domain::value_objects::awb_generator::{AwbGenerator, AwbGeneratorError};

/// Composite generator: tries Redis first, falls back to PostgreSQL on error.
///
/// This is the production implementation injected into `ShipmentService`.
/// The fallback is transparent to callers — they only see `AwbGenerator`.
pub struct FallbackAwbGenerator {
    primary:  Arc<dyn AwbGenerator>,
    fallback: Arc<dyn AwbGenerator>,
}

impl FallbackAwbGenerator {
    pub fn new(primary: Arc<dyn AwbGenerator>, fallback: Arc<dyn AwbGenerator>) -> Self {
        Self { primary, fallback }
    }
}

#[async_trait]
impl AwbGenerator for FallbackAwbGenerator {
    async fn next_awb(
        &self,
        tenant_code: &TenantCode,
        service: ServiceCode,
    ) -> Result<Awb, AwbGeneratorError> {
        match self.primary.next_awb(tenant_code, service).await {
            Ok(awb) => Ok(awb),
            Err(e) => {
                error!(
                    error = %e,
                    tenant = tenant_code.as_str(),
                    "Primary AWB generator failed; switching to fallback"
                );
                self.fallback.next_awb(tenant_code, service).await
            }
        }
    }
}
