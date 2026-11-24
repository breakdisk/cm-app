//! HTTP client for order-intake's internal shipment endpoints.
//!
//! Implements one driven port:
//! - `ShipmentBillingContextSource` — `GET /v1/internal/shipments/:id/billing`
//!
//! No JWT is attached: the `/v1/internal/*` routes are mTLS-only by design
//! (Istio enforces that only in-mesh services can reach them), matching how
//! payments' `OrderIntakeClient` calls the same endpoint.

use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::repositories::{ShipmentBillingContext, ShipmentBillingContextSource};

/// Wall-clock ceiling for the lookup. A hung order-intake becomes a fast error,
/// which initiate turns into a 503 (the driver app retries), rather than a hung
/// request.
const LOOKUP_TIMEOUT_SECS: u64 = 5;

pub struct OrderIntakeClient {
    base_url: String,
    http:     reqwest::Client,
}

impl OrderIntakeClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            http:     reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(LOOKUP_TIMEOUT_SECS))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        }
    }
}

/// Subset of `GET /v1/internal/shipments/:id/billing` that POP cares about.
/// The endpoint also returns the computed fee breakdown, which is ignored here.
#[derive(Debug, serde::Deserialize)]
struct BillingContextResponse {
    #[serde(default)]
    service_code:         Option<String>,
    #[serde(default)]
    declared_value_cents: Option<i64>,
    #[serde(default)]
    customer_phone:       Option<String>,
    /// The booking's tenant. Absent from an order-intake that predates it.
    #[serde(default)]
    tenant_id:            Option<Uuid>,
}

#[async_trait]
impl ShipmentBillingContextSource for OrderIntakeClient {
    async fn fetch(&self, tenant_id: Uuid, shipment_id: Uuid) -> anyhow::Result<ShipmentBillingContext> {
        let url = format!(
            "{}/v1/internal/shipments/{}/billing",
            self.base_url.trim_end_matches('/'),
            shipment_id,
        );

        let resp = self.http
            .get(&url)
            .send()
            .await?;
        // An explicit 404 is the one answer that says the shipment does not
        // exist; every other failure is an outage and must not block capture.
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(anyhow::Error::new(crate::domain::repositories::ShipmentNotFound(shipment_id)));
        }
        let resp = resp
            .error_for_status()?
            .json::<BillingContextResponse>()
            .await?;

        context_from_response(tenant_id, shipment_id, resp)
    }
}

/// Maps order-intake's answer to the context, enforcing tenant ownership.
///
/// - the booking's tenant differs from the caller's: [`ShipmentNotFound`], the
///   same error a 404 produces, so a caller cannot tell "foreign" from "missing";
/// - no `tenant_id` in the response: an order-intake that predates the field.
///   That is an outage, not a pass: ownership cannot be shown, so it must never
///   be read as "owned";
/// - a missing `service_code` is likewise an error rather than a silent
///   classification of a Balikbayan parcel as "standard".
fn context_from_response(
    tenant_id:   Uuid,
    shipment_id: Uuid,
    resp:        BillingContextResponse,
) -> anyhow::Result<ShipmentBillingContext> {
    match resp.tenant_id {
        Some(owner) if owner != tenant_id => {
            return Err(anyhow::Error::new(crate::domain::repositories::ShipmentNotFound(shipment_id)));
        }
        Some(_) => {}
        None => anyhow::bail!(
            "order-intake returned no tenant_id for shipment {shipment_id} \
             (deployed version predates tenant_id); ownership cannot be verified"
        ),
    }

    let service_code = resp.service_code.ok_or_else(|| {
        anyhow::anyhow!(
            "order-intake returned no service_code for shipment {shipment_id} \
             (deployed version predates the POP billing-context fields?)"
        )
    })?;

    Ok(ShipmentBillingContext {
        service_code,
        declared_value_cents: resp.declared_value_cents,
        customer_phone:       resp.customer_phone,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resp(tenant: Option<Uuid>) -> BillingContextResponse {
        BillingContextResponse {
            service_code: Some("standard".into()),
            declared_value_cents: None,
            customer_phone: Some("+639171234567".into()),
            tenant_id: tenant,
        }
    }

    #[test]
    fn own_tenant_maps_to_the_context() {
        let t = Uuid::new_v4();
        let ctx = context_from_response(t, Uuid::new_v4(), resp(Some(t))).expect("owned");
        assert_eq!(ctx.service_code, "standard");
    }

    #[test]
    fn another_tenants_shipment_is_shipment_not_found() {
        let err = context_from_response(Uuid::new_v4(), Uuid::new_v4(), resp(Some(Uuid::new_v4()))).unwrap_err();
        assert!(err.downcast_ref::<crate::domain::repositories::ShipmentNotFound>().is_some());
    }

    #[test]
    fn a_response_with_no_tenant_id_is_an_outage_not_a_pass() {
        let err = context_from_response(Uuid::new_v4(), Uuid::new_v4(), resp(None)).unwrap_err();
        assert!(
            err.downcast_ref::<crate::domain::repositories::ShipmentNotFound>().is_none(),
            "must not read as an explicit refusal"
        );
        assert!(err.to_string().contains("predates tenant_id"), "{err}");
    }
}
