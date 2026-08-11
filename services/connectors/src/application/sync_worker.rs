//! The unattended half of the catalog sync.
//!
//! `POST /v1/connectors/catalog/sync` is a merchant pressing a button. This is
//! the same work on a schedule, for vendors who set an interval — and it is the
//! last piece that makes the ingest port worth having, because a catalog synced
//! once by hand goes stale the same day.
//!
//! ## What it is not allowed to do
//!
//! Confirm stock. Nothing here touches `confirmed_at`; that is enforced two
//! layers down in `CatalogItem::merge_ingested`, and the whole reason an
//! unattended sweep is safe to run at all. A nightly sync updates prices and
//! listings and leaves every item needing a human — which is correct, and is
//! why the console leads with what needs confirming.
//!
//! ## Why claiming, not listing
//!
//! Two replicas is the normal state under rolling updates. `claim_due_syncs`
//! stamps and locks in one statement so only one sweep takes a given connector;
//! see the repository for the mechanics and `tests/sync_claim.rs` for the proof.

use std::sync::Arc;
use std::time::Duration;

use uuid::Uuid;

use crate::domain::entities::ConnectorCredentials;
use crate::domain::repositories::CredentialsRepository;
use crate::infrastructure::omnideliv_client::OmniDelivClient;

/// How many connectors one tick will take. A ceiling rather than a target: each
/// one is several outbound HTTP calls to someone else's shop, and a sweep that
/// grabbed 500 at once would hold them all in flight.
const BATCH: i64 = 10;

pub struct SyncWorker {
    creds:     Arc<dyn CredentialsRepository>,
    omnideliv: Arc<OmniDelivClient>,
    http:      reqwest::Client,
    tick:      Duration,
}

impl SyncWorker {
    pub fn new(
        creds: Arc<dyn CredentialsRepository>,
        omnideliv: Arc<OmniDelivClient>,
        http: reqwest::Client,
        tick_secs: u64,
    ) -> Self {
        Self { creds, omnideliv, http, tick: Duration::from_secs(tick_secs) }
    }

    /// Run until the process ends.
    ///
    /// Never returns an error: a sweep that gave up on a transient database
    /// blip would leave every scheduled sync silently dead until the next
    /// deploy, which is exactly the failure this whole feature is meant to
    /// prevent someone discovering weeks later.
    pub async fn run(self) {
        let mut ticker = tokio::time::interval(self.tick);
        // A missed tick must not queue up and then fire a burst — under load
        // that turns one slow sweep into several concurrent ones.
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        tracing::info!(tick_secs = self.tick.as_secs(), "catalog sync worker started");

        loop {
            ticker.tick().await;
            match self.creds.claim_due_syncs(BATCH).await {
                Ok(due) if due.is_empty() => {}
                Ok(due) => {
                    tracing::info!(count = due.len(), "claimed connectors for catalog sync");
                    for creds in due {
                        // One failure must not abandon the rest of the batch:
                        // a single vendor with revoked credentials would
                        // otherwise stop every other vendor's sync.
                        self.sync_one(&creds).await;
                    }
                }
                Err(e) => tracing::error!(err = %e, "could not claim due catalog syncs"),
            }
        }
    }

    async fn sync_one(&self, creds: &ConnectorCredentials) {
        let id = creds.id;
        let outcome = self.fetch_and_ingest(creds).await;

        let error = match &outcome {
            Ok(report) => {
                tracing::info!(
                    connector_id = %id, platform = creds.platform.as_str(),
                    created = report.created, updated = report.updated,
                    rejected = report.rejected,
                    "scheduled catalog sync applied",
                );
                None
            }
            Err(e) => {
                // Warn, not error: a merchant rotating their API token is an
                // ordinary event, and paging on it would train people to
                // ignore the channel.
                tracing::warn!(
                    connector_id = %id, platform = creds.platform.as_str(), err = %e,
                    "scheduled catalog sync failed",
                );
                Some(e.to_string())
            }
        };

        // Recorded on the row so a sync that has been failing for a week is
        // visible without reading logs.
        if let Err(e) = self.creds.record_sync_result(id, error.as_deref()).await {
            tracing::error!(connector_id = %id, err = %e, "could not record sync result");
        }
    }

    async fn fetch_and_ingest(
        &self,
        creds: &ConnectorCredentials,
    ) -> anyhow::Result<crate::infrastructure::omnideliv_client::IngestReport> {
        let vendor_id: Uuid = creds.omnideliv_vendor_id().ok_or_else(|| {
            // Enabling a schedule without saying which store to sync into is a
            // configuration mistake, and it should sit visibly on the row
            // rather than being retried silently every hour.
            anyhow::anyhow!("no omnideliv_vendor_id configured on this connector")
        })?;

        let items = match creds.platform.as_str() {
            "shopify" => crate::adapters::shopify_catalog::fetch_products(&self.http, creds).await?,
            "woocommerce" => {
                let m = crate::adapters::woocommerce_catalog::fetch_products(&self.http, creds).await?;
                if m.deferred_variable > 0 || m.unpriced > 0 {
                    tracing::warn!(
                        connector_id = %creds.id,
                        deferred = m.deferred_variable, unpriced = m.unpriced,
                        "scheduled sync could not bring over every row",
                    );
                }
                m.items
            }
            other => anyhow::bail!("no catalog adapter for platform {other}"),
        };

        Ok(self
            .omnideliv
            .ingest_catalog(
                creds.tenant_id,
                &creds.tenant_slug,
                vendor_id,
                creds.platform.as_str(),
                &items,
            )
            .await?)
    }
}
