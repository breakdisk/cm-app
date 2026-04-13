//! Kafka consumer for `logisticos.pod.captured`.
//!
//! On each event:
//!  1. If COD was collected (`cod_amount_cents > 0`), calls `CodService::reconcile_cod`.
//!  2. If the shipment was booked by a customer (`booked_by_customer == true`),
//!     calls `InvoiceService::issue_payment_receipt` to issue and capture a
//!     per-shipment payment receipt (Draft → Issued → Paid in one step).

use std::sync::Arc;
use anyhow::Context;
use logisticos_events::{
    consumer::KafkaConsumer,
    payloads::PodCaptured,
    topics::POD_CAPTURED,
};
use logisticos_types::TenantId;

use crate::application::{
    commands::{IssuePaymentReceiptCommand, ReconcileCodCommand},
    services::{CodService, InvoiceService},
};

pub struct PodConsumer {
    inner:           KafkaConsumer,
    cod_service:     Arc<CodService>,
    invoice_service: Arc<InvoiceService>,
}

impl PodConsumer {
    pub fn new(
        brokers:         &str,
        group_id:        &str,
        cod_service:     Arc<CodService>,
        invoice_service: Arc<InvoiceService>,
    ) -> anyhow::Result<Self> {
        let inner = KafkaConsumer::new(brokers, group_id, &[POD_CAPTURED])
            .context("Failed to create PodConsumer Kafka client")?;
        Ok(Self { inner, cod_service, invoice_service })
    }

    pub async fn run(self) {
        let cod_service     = self.cod_service;
        let invoice_service = self.invoice_service;

        let result = self.inner.run(move |_topic, json| {
            let cod = Arc::clone(&cod_service);
            let inv = Arc::clone(&invoice_service);
            async move { handle_pod_captured(cod, inv, json).await }
        }).await;

        if let Err(e) = result {
            tracing::error!("PodConsumer loop exited with error: {e}");
        }
    }
}

async fn handle_pod_captured(
    cod_service:     Arc<CodService>,
    invoice_service: Arc<InvoiceService>,
    json:            serde_json::Value,
) -> anyhow::Result<()> {
    // Deserialise from envelope: payload lives under "data" key.
    let payload: PodCaptured = serde_json::from_value(
        json.get("data").cloned().unwrap_or(json.clone()),
    )
    .context("Failed to deserialise PodCaptured payload")?;

    let tenant_id_raw = json.get("tenant_id").and_then(|v| v.as_str())
        .or_else(|| {
            json.get("data")
                .and_then(|d| d.get("tenant_id"))
                .and_then(|v| v.as_str())
        });

    // ── COD reconciliation ────────────────────────────────────────────────────
    if payload.cod_amount_cents > 0 {
        let tenant_id = parse_tenant_id(tenant_id_raw, payload.pod_id)?;
        tracing::info!(
            pod_id      = %payload.pod_id,
            shipment_id = %payload.shipment_id,
            cod_cents   = payload.cod_amount_cents,
            "Reconciling COD from pod.captured"
        );
        cod_service
            .reconcile_cod(
                &tenant_id,
                ReconcileCodCommand {
                    shipment_id:  payload.shipment_id,
                    pod_id:       payload.pod_id,
                    driver_id:    payload.driver_id,
                    amount_cents: payload.cod_amount_cents,
                },
            )
            .await
            .map_err(|e| anyhow::anyhow!("COD reconciliation failed: {e:?}"))?;
    } else {
        tracing::debug!(pod_id = %payload.pod_id, "POD captured — no COD");
    }

    // ── Payment receipt for customer-booked shipments ─────────────────────────
    if payload.booked_by_customer {
        let tenant_id = parse_tenant_id(tenant_id_raw, payload.pod_id)?;

        let customer_id = payload.customer_id
            .context("pod.captured missing customer_id for customer-booked shipment")?;

        let delivered_on = chrono::DateTime::parse_from_rfc3339(&payload.captured_at)
            .map(|dt| dt.date_naive())
            .unwrap_or_else(|_| chrono::Utc::now().date_naive());

        let tenant_code = if payload.tenant_code.is_empty() {
            "PH1".to_owned()
        } else {
            payload.tenant_code.clone()
        };

        tracing::info!(
            pod_id      = %payload.pod_id,
            shipment_id = %payload.shipment_id,
            customer_id = %customer_id,
            "Issuing payment receipt for customer-booked delivery"
        );

        invoice_service
            .issue_payment_receipt(
                &tenant_id,
                IssuePaymentReceiptCommand {
                    shipment_id:    payload.shipment_id,
                    tenant_code,
                    customer_id,
                    customer_email: payload.customer_email.clone(),
                    delivered_on,
                },
            )
            .await
            .map_err(|e| anyhow::anyhow!("Payment receipt issuance failed: {e:?}"))?;
    }

    Ok(())
}

fn parse_tenant_id(raw: Option<&str>, pod_id: uuid::Uuid) -> anyhow::Result<TenantId> {
    match raw {
        Some(id) => id.parse::<uuid::Uuid>()
            .context("Invalid tenant_id UUID in pod.captured")
            .map(TenantId::from_uuid),
        None => {
            tracing::warn!(pod_id = %pod_id, "pod.captured missing tenant_id");
            anyhow::bail!("pod.captured missing tenant_id")
        }
    }
}
