//! ConnectorService — orchestrates HMAC verification, payload mapping, shipment
//! creation, and bidirectional tracking push-back for Shopify and WooCommerce.

use std::sync::Arc;

use axum::body::Bytes;
use logisticos_errors::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    adapters::{shopify, woocommerce},
    domain::repositories::CredentialsRepository,
    infrastructure::{hmac, order_intake_client::OrderIntakeClient},
};

/// Request body for creating / updating connector credentials (authenticated endpoint).
#[derive(Debug, Deserialize)]
pub struct UpsertCredentialsCommand {
    pub platform:        String,   // "shopify" | "woocommerce"
    pub webhook_secret:  String,
    pub config:          serde_json::Value,
}

/// Public summary returned from the list/create credentials endpoints.
#[derive(Debug, Serialize)]
pub struct CredentialsSummary {
    pub id:              Uuid,
    pub platform:        String,
    pub is_active:       bool,
    pub webhook_url:     String,   // pre-constructed URL for merchant to paste into platform
    /// `None` = never synced. The console shows that as its own state rather
    /// than as a blank, because "connected but nothing pulled yet" is the case
    /// a merchant is most likely to be staring at and least able to diagnose.
    pub last_synced_at:  Option<chrono::DateTime<chrono::Utc>>,
    /// Minutes between scheduled sweeps, so the console can tell overdue from
    /// merely recent without hard-coding the schedule. `None` = no schedule;
    /// this connection only moves when someone presses Sync.
    pub sync_interval_mins: Option<i32>,
    pub created_at:      chrono::DateTime<chrono::Utc>,
}

pub struct ConnectorService {
    pub creds_repo:   Arc<dyn CredentialsRepository>,
    pub order_intake: Arc<OrderIntakeClient>,
    pub http:         reqwest::Client,
}

impl ConnectorService {
    pub fn new(
        creds_repo:   Arc<dyn CredentialsRepository>,
        order_intake: Arc<OrderIntakeClient>,
    ) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("failed to build outbound HTTP client");
        Self { creds_repo, order_intake, http }
    }

    // ── Shopify ───────────────────────────────────────────────────────────────

    /// Handle an inbound Shopify webhook.
    ///
    /// Flow:
    /// 1. Load credentials → HMAC verify → parse → map → create shipment
    /// 2. Fire-and-forget: push AWB back to Shopify Admin API
    pub async fn handle_shopify_webhook(
        &self,
        tenant_id:   Uuid,
        topic:       &str,
        body:        &Bytes,
        hmac_header: &str,
    ) -> AppResult<()> {
        // Only process new order creation events; ACK all others immediately.
        if topic != "orders/create" {
            tracing::debug!(topic, "shopify: ignoring non-create topic");
            return Ok(());
        }

        // Load credentials — missing config means this tenant hasn't set up Shopify.
        let creds = self
            .creds_repo
            .find(tenant_id, "shopify")
            .await?
            .ok_or_else(|| AppError::Unauthorized(
                "Shopify connector not configured for this tenant".into(),
            ))?;

        // Verify HMAC before touching the payload.
        if !hmac::verify_shopify(creds.webhook_secret.as_bytes(), body, hmac_header) {
            tracing::warn!(tenant_id = %tenant_id, "shopify: HMAC mismatch");
            return Err(AppError::Unauthorized("HMAC signature mismatch".into()));
        }

        // Parse the Shopify order payload.
        let order: shopify::ShopifyOrder = serde_json::from_slice(body).map_err(|e| {
            AppError::Validation(format!("failed to parse Shopify order payload: {e}"))
        })?;

        let order_id = order.id;
        let is_cod = order
            .gateway
            .as_deref()
            .map(|gw| creds.shopify_cod_gateways().contains(&gw))
            .unwrap_or(false);

        // Map to internal command.
        let cmd = shopify::map_to_command(&order, &creds)?;

        // Create shipment via order-intake internal endpoint.
        let created = self.order_intake.create_shipment(&cmd).await?;

        tracing::info!(
            awb        = %created.awb,
            order_id   = order_id,
            is_cod     = is_cod,
            tenant_id  = %tenant_id,
            "shopify: shipment created"
        );

        // Push tracking back to Shopify — fire-and-forget.
        let http    = self.http.clone();
        let awb     = created.awb.clone();
        let creds_c = creds.clone();
        let tracking_url = format!(
            "https://track.cargomarket.net/{}",
            created.awb
        );
        tokio::spawn(async move {
            if let Err(e) = shopify::push_tracking_back(
                &http, &creds_c, order_id, &awb, &tracking_url, is_cod,
            ).await {
                tracing::error!(error = %e, awb = %awb, "shopify: tracking push-back failed");
            } else {
                tracing::info!(awb = %awb, "shopify: tracking push-back succeeded");
            }
        });

        Ok(())
    }

    // ── WooCommerce ───────────────────────────────────────────────────────────

    /// Handle an inbound WooCommerce webhook.
    pub async fn handle_woocommerce_webhook(
        &self,
        tenant_id:        Uuid,
        event:            &str,
        body:             &Bytes,
        sig_header:       &str,
    ) -> AppResult<()> {
        // WooCommerce sends X-WC-Webhook-Topic: "order.created"
        if event != "order.created" {
            tracing::debug!(event, "woocommerce: ignoring non-create event");
            return Ok(());
        }

        let creds = self
            .creds_repo
            .find(tenant_id, "woocommerce")
            .await?
            .ok_or_else(|| AppError::Unauthorized(
                "WooCommerce connector not configured for this tenant".into(),
            ))?;

        if !hmac::verify_woocommerce(creds.webhook_secret.as_bytes(), body, sig_header) {
            tracing::warn!(tenant_id = %tenant_id, "woocommerce: HMAC mismatch");
            return Err(AppError::Unauthorized("HMAC signature mismatch".into()));
        }

        let order: woocommerce::WooCommerceOrder =
            serde_json::from_slice(body).map_err(|e| {
                AppError::Validation(format!("failed to parse WooCommerce order payload: {e}"))
            })?;

        let order_id = order.id;
        let is_cod = creds
            .woo_cod_payment_methods().contains(&order.payment_method.as_str());

        let cmd = woocommerce::map_to_command(&order, &creds)?;
        let created = self.order_intake.create_shipment(&cmd).await?;

        tracing::info!(
            awb       = %created.awb,
            order_id  = order_id,
            is_cod    = is_cod,
            tenant_id = %tenant_id,
            "woocommerce: shipment created"
        );

        // Push tracking back — fire-and-forget.
        let http    = self.http.clone();
        let awb     = created.awb.clone();
        let creds_c = creds.clone();
        let tracking_url = format!("https://track.cargomarket.net/{}", created.awb);
        tokio::spawn(async move {
            if let Err(e) = woocommerce::push_tracking_back(
                &http, &creds_c, order_id, &awb, &tracking_url, is_cod,
            ).await {
                tracing::error!(error = %e, awb = %awb, "woocommerce: tracking push-back failed");
            } else {
                tracing::info!(awb = %awb, "woocommerce: tracking push-back succeeded");
            }
        });

        Ok(())
    }

    // ── Credentials management (authenticated) ────────────────────────────────

    pub async fn list_credentials(
        &self,
        tenant_id:  Uuid,
        public_url: &str,
    ) -> AppResult<Vec<CredentialsSummary>> {
        let all = self.creds_repo.list_for_tenant(tenant_id).await?;
        Ok(all
            .into_iter()
            .map(|c| CredentialsSummary {
                id:          c.id,
                platform:    c.platform.as_str().to_string(),
                is_active:   c.is_active,
                webhook_url: format!(
                    "{}/v1/connectors/{}/{}/webhook",
                    public_url.trim_end_matches('/'),
                    c.platform.as_str(),
                    c.tenant_id,
                ),
                last_synced_at: c.last_synced_at,
                sync_interval_mins: c.sync_interval_mins,
                created_at: c.created_at,
            })
            .collect())
    }

    pub async fn delete_credentials(
        &self,
        tenant_id: Uuid,
        platform:  &str,
    ) -> AppResult<()> {
        self.creds_repo.delete(tenant_id, platform).await
    }
}
