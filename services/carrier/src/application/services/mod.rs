pub mod marketplace;

pub use marketplace::{
    AuthorizedIntent, BookingError, BookingPayments, CreateBookingCommand, CreateListingCommand,
    MarketplaceService, PlacedBooking, RecordPickupInput, UpdateListingPatch,
};

use std::sync::Arc;
use serde::Deserialize;
use rand::RngCore;
use sha2::{Sha256, Digest};
use uuid::Uuid;

use logisticos_errors::{AppError, AppResult};
use logisticos_types::TenantId;

use crate::{
    domain::{
        entities::{Carrier, CarrierId, RateCard, SlaCommitment, SlaRecord, ZoneSlaRow},
        repositories::{CarrierRepository, SlaRecordRepository},
    },
    infrastructure::messaging::CarrierPublisher,
};

#[derive(Debug, Deserialize)]
pub struct OnboardCarrierCommand {
    pub name:          String,
    pub code:          String,
    pub contact_email: String,
    pub sla_target:    f64,
    pub max_delivery_days: u8,
}

/// Partial update applied to an existing carrier — fields left None are
/// preserved. Used by partner-portal Settings (profile fields) and Rates
/// (rate_cards).
///
/// Authorization lives in `application::authz`: `carriers:manage` reaches any
/// carrier in the tenant, `carriers:manage-own` only the caller's own record.
/// The full partner_id model is still ADR-0013's job; contact-email identity is
/// the interim binding.
#[derive(Debug, Deserialize)]
pub struct UpdateCarrierCommand {
    pub name:              Option<String>,
    pub contact_email:     Option<String>,
    pub contact_phone:     Option<String>,
    pub api_endpoint:      Option<String>,
    pub sla:               Option<SlaCommitment>,
    pub rate_cards:        Option<Vec<RateCard>>,
    /// The KYB verdict. Requires full `carriers:manage` — the HTTP handler
    /// rejects this field for `carriers:manage-own` callers so a partner
    /// cannot mark itself `compliant`.
    pub compliance_status: Option<crate::domain::entities::ComplianceStatus>,
}

/// Rate shopping result — returned to the dispatch service to choose carrier for a shipment.
#[derive(Debug, serde::Serialize)]
pub struct CarrierQuote {
    pub carrier_id:           Uuid,
    pub carrier_name:         String,
    pub carrier_code:         String,
    pub service_type:         String,
    /// Total cost in cents (base_rate + per_kg * weight). Renamed from `total_cents`
    /// to match the frontend and dispatch service contract.
    pub total_cost_cents:     i64,
    pub on_time_rate:         f64,
    pub grade:                crate::domain::entities::PerformanceGrade,
    /// Always true for quotes returned from rate-shop; false would indicate a
    /// carrier with no matching rate card (filtered before this point).
    pub eligible:             bool,
    pub ineligibility_reason: Option<String>,
}

pub struct CarrierService {
    repo:            Arc<dyn CarrierRepository>,
    sla_repo:        Arc<dyn SlaRecordRepository>,
    publisher:       Arc<CarrierPublisher>,
    external_client: Arc<crate::infrastructure::external::ExternalCarrierClient>,
}

impl CarrierService {
    pub fn new(
        repo:            Arc<dyn CarrierRepository>,
        sla_repo:        Arc<dyn SlaRecordRepository>,
        publisher:       Arc<CarrierPublisher>,
        external_client: Arc<crate::infrastructure::external::ExternalCarrierClient>,
    ) -> Self {
        Self { repo, sla_repo, publisher, external_client }
    }

    pub async fn onboard(&self, tenant_id: &TenantId, cmd: OnboardCarrierCommand) -> AppResult<Carrier> {
        // Validate code uniqueness within tenant.
        if self.repo.find_by_code(tenant_id, &cmd.code).await.map_err(AppError::internal)?.is_some() {
            return Err(AppError::BusinessRule(format!("Carrier code '{}' already registered", cmd.code)));
        }
        let sla = SlaCommitment {
            on_time_target_pct: cmd.sla_target.clamp(0.0, 100.0),
            max_delivery_days: cmd.max_delivery_days,
            penalty_per_breach: 0,
        };
        let carrier = Carrier::new(tenant_id.clone(), cmd.name.clone(), cmd.code.clone(), cmd.contact_email.clone(), sla);
        self.repo.save(&carrier).await.map_err(AppError::internal)?;

        if let Err(e) = self.publisher.carrier_onboarded(
            carrier.id.inner(),
            carrier.tenant_id.inner(),
            carrier.name.clone(),
            carrier.code.clone(),
            carrier.contact_email.clone(),
        ).await {
            tracing::warn!("Failed to publish carrier_onboarded event: {e}");
        }
        Ok(carrier)
    }

    pub async fn activate(&self, id: Uuid) -> AppResult<Carrier> {
        let mut carrier = self.get(id).await?;
        let old_status = format!("{:?}", carrier.status).to_lowercase();
        carrier.activate().map_err(|e| AppError::BusinessRule(e.to_string()))?;
        self.repo.save(&carrier).await.map_err(AppError::internal)?;

        if let Err(e) = self.publisher.carrier_status_changed(
            carrier.id.inner(), carrier.tenant_id.inner(),
            old_status, "active".into(), String::new(),
        ).await {
            tracing::warn!("Failed to publish carrier_status_changed (activate): {e}");
        }
        Ok(carrier)
    }

    pub async fn suspend(&self, id: Uuid, reason: String) -> AppResult<Carrier> {
        let mut carrier = self.get(id).await?;
        let old_status = format!("{:?}", carrier.status).to_lowercase();
        carrier.suspend(&reason);
        self.repo.save(&carrier).await.map_err(AppError::internal)?;

        if let Err(e) = self.publisher.carrier_status_changed(
            carrier.id.inner(), carrier.tenant_id.inner(),
            old_status, "suspended".into(), reason,
        ).await {
            tracing::warn!("Failed to publish carrier_status_changed (suspend): {e}");
        }
        Ok(carrier)
    }

    /// Apply a partial update — name/contact/sla/rate_cards. Tenant
    /// isolation is enforced by the caller (the HTTP handler asserts
    /// claims.tenant_id == carrier.tenant_id before invoking).
    pub async fn update(&self, id: Uuid, cmd: UpdateCarrierCommand) -> AppResult<Carrier> {
        let mut carrier = self.get(id).await?;
        if let Some(v) = cmd.name           { carrier.name = v; }
        if let Some(v) = cmd.contact_email  { carrier.contact_email = v; }
        if let Some(v) = cmd.contact_phone  { carrier.contact_phone = Some(v); }
        if let Some(v) = cmd.api_endpoint   { carrier.api_endpoint = Some(v); }
        if let Some(v) = cmd.sla {
            // Clamp + sanity so a slip in the UI can't post 110% targets.
            carrier.sla = SlaCommitment {
                on_time_target_pct: v.on_time_target_pct.clamp(0.0, 100.0),
                max_delivery_days:  v.max_delivery_days.max(1),
                penalty_per_breach: v.penalty_per_breach.max(0),
            };
        }
        if let Some(v) = cmd.rate_cards        { carrier.rate_cards = v; }
        if let Some(v) = cmd.compliance_status { carrier.compliance_status = v; }
        carrier.updated_at = chrono::Utc::now();
        self.repo.save(&carrier).await.map_err(AppError::internal)?;
        Ok(carrier)
    }

    pub async fn get(&self, id: Uuid) -> AppResult<Carrier> {
        self.repo
            .find_by_id(&CarrierId::from_uuid(id))
            .await
            .map_err(AppError::internal)?
            .ok_or_else(|| AppError::NotFound { resource: "Carrier", id: id.to_string() })
    }

    /// Look up the carrier whose contact_email matches the authenticated user's
    /// email — used by the partner portal's GET /v1/carriers/me endpoint.
    pub async fn get_by_email(&self, tenant_id: &TenantId, email: &str) -> AppResult<Carrier> {
        self.repo
            .find_by_contact_email(tenant_id, email)
            .await
            .map_err(AppError::internal)?
            .ok_or_else(|| AppError::NotFound { resource: "Carrier", id: email.to_string() })
    }

    pub async fn list(&self, tenant_id: &TenantId, limit: i64, offset: i64) -> AppResult<Vec<Carrier>> {
        self.repo.list(tenant_id, limit.clamp(1, 100), offset.max(0)).await.map_err(AppError::internal)
    }

    /// List all carriers in `active` status — used by rate-shop and the gRPC service.
    pub async fn list_active(&self, tenant_id: &TenantId) -> AppResult<Vec<Carrier>> {
        self.repo.list_active(tenant_id).await.map_err(AppError::internal)
    }

    /// Rate shop: return quotes from all active carriers for a given service type and weight.
    /// Results sorted by total cost ascending (cheapest first).
    pub async fn shop_rates(
        &self,
        tenant_id: &TenantId,
        service_type: &str,
        weight_kg: f32,
    ) -> AppResult<Vec<CarrierQuote>> {
        let carriers = self.repo.list_active(tenant_id).await.map_err(AppError::internal)?;

        let mut quotes: Vec<CarrierQuote> = carriers
            .into_iter()
            .filter_map(|c| {
                c.quote(service_type, weight_kg).map(|total| CarrierQuote {
                    carrier_id:           c.id.inner(),
                    carrier_name:         c.name.clone(),
                    carrier_code:         c.code.clone(),
                    service_type:         service_type.to_owned(),
                    total_cost_cents:     total,
                    on_time_rate:         c.on_time_rate(),
                    grade:                c.performance_grade.clone(),
                    eligible:             true,
                    ineligibility_reason: None,
                })
            })
            .collect();

        quotes.sort_by_key(|q| q.total_cost_cents);
        Ok(quotes)
    }

    /// Called by `POST /v1/internal/sla-records` when dispatch allocates a carrier
    /// to a shipment. Creates the SLA record and publishes a `carrier.allocated` event.
    ///
    /// The carrier is loaded first and must belong to `record.tenant_id`. A carrier
    /// that does not exist and one that belongs to another tenant are refused with
    /// the same `NotFound`, so a caller cannot probe which carrier ids exist in
    /// other tenants. Nothing is written, published or sent before that check.
    pub async fn create_sla_record(
        &self,
        record: SlaRecord,
        total_cost_cents: i64,
        method: String,
    ) -> AppResult<SlaRecord> {
        let carrier = self
            .repo
            .find_by_id(&CarrierId::from_uuid(record.carrier_id))
            .await
            .map_err(AppError::internal)?
            .filter(|c| c.tenant_id.inner() == record.tenant_id)
            .ok_or_else(|| AppError::NotFound {
                resource: "Carrier",
                id: record.carrier_id.to_string(),
            })?;

        self.sla_repo.create(&record).await.map_err(AppError::internal)?;

        let payload = logisticos_events::payloads::CarrierAllocated {
            carrier_id:       record.carrier_id,
            tenant_id:        record.tenant_id,
            shipment_id:      record.shipment_id,
            zone:             record.zone.clone(),
            service_level:    record.service_level.clone(),
            total_cost_cents,
            promised_by:      record.promised_by.to_rfc3339(),
            method,
        };
        if let Err(e) = self.publisher.carrier_allocated(record.tenant_id, payload).await {
            tracing::warn!("Failed to publish carrier_allocated event: {e}");
        }

        // Push outbound webhook to the carrier's registered api_endpoint (best-effort).
        if let Some(ref endpoint) = carrier.api_endpoint {
            let notif = crate::infrastructure::external::CarrierShipmentNotification {
                event:           "shipment.allocated",
                shipment_id:     record.shipment_id.to_string(),
                awb:             String::new(),
                service_type:    record.service_level.clone(),
                zone:            record.zone.clone(),
                promised_by:     record.promised_by.to_rfc3339(),
                pickup_address:  None,
                dropoff_address: None,
                weight_kg:       None,
            };
            self.external_client.notify_shipment_allocated(endpoint, &notif).await;
        }

        Ok(record)
    }

    /// Zone-level SLA summary for a carrier over a time window.
    pub async fn sla_zone_summary(
        &self,
        carrier_id: Uuid,
        from: chrono::DateTime<chrono::Utc>,
        to: chrono::DateTime<chrono::Utc>,
    ) -> AppResult<Vec<ZoneSlaRow>> {
        self.sla_repo.zone_summary(carrier_id, from, to).await.map_err(AppError::internal)
    }

    /// Generate (or rotate) an API key for a carrier.
    ///
    /// Returns the raw plaintext key **once** — the caller must display it
    /// immediately and inform the partner to copy it. Only the SHA-256 hash
    /// is persisted. Rotating overwrites the previous hash, invalidating the
    /// old key instantly.
    ///
    /// Key format: `los_<64-char hex>` (32 random bytes, hex-encoded).
    pub async fn generate_api_key(&self, id: Uuid) -> AppResult<String> {
        let mut carrier = self.get(id).await?;

        // 32 bytes from OsRng — explicit OS-entropy path, immune to any
        // thread_rng feature-flag changes. Produces a 64-char hex suffix.
        let mut raw_bytes = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut raw_bytes);
        let raw_key = format!("los_{}", raw_bytes.iter().map(|b| format!("{b:02x}")).collect::<String>());

        // Store only the SHA-256 hash — the plaintext is never persisted.
        let hash = {
            let mut hasher = Sha256::new();
            hasher.update(raw_key.as_bytes());
            format!("{:x}", hasher.finalize())
        };

        carrier.api_key_hash = Some(hash);
        carrier.has_api_key  = true;
        carrier.updated_at   = chrono::Utc::now();
        self.repo.save(&carrier).await.map_err(AppError::internal)?;

        Ok(raw_key)
    }

    /// Publish an inbound 3PL tracking event to Kafka (G3 — webhook wiring).
    /// Called by the carrier_tracking_webhook HTTP handler after API key auth.
    pub async fn publish_tracking_event(
        &self,
        carrier_id: Uuid,
        tenant_id: Uuid,
        carrier_ref: Option<String>,
        shipment_id: Option<uuid::Uuid>,
        event: String,
        message: Option<String>,
    ) -> AppResult<()> {
        let payload = logisticos_events::payloads::CarrierTrackingEvent {
            carrier_id,
            tenant_id,
            carrier_ref,
            shipment_id,
            event,
            message,
            received_at: chrono::Utc::now().to_rfc3339(),
        };
        if let Err(e) = self.publisher.carrier_tracking_event(tenant_id, payload).await {
            tracing::warn!("Failed to publish carrier_tracking_event: {e}");
        }
        Ok(())
    }

    /// Expose repo handles so `CarrierCommandHandler` can reach them
    /// without duplicating the Kafka consumer's outcome logic.
    pub fn sla_repo(&self) -> &Arc<dyn crate::domain::repositories::SlaRecordRepository> {
        &self.sla_repo
    }

    pub fn carrier_repo(&self) -> &Arc<dyn crate::domain::repositories::CarrierRepository> {
        &self.repo
    }

    /// Paginated SLA record history for a carrier (partner portal).
    pub async fn sla_history(
        &self,
        carrier_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> AppResult<Vec<SlaRecord>> {
        self.sla_repo
            .list_by_carrier(carrier_id, limit.clamp(1, 100), offset.max(0))
            .await
            .map_err(AppError::internal)
    }

    /// Aggregate failed deliveries by reason for a carrier over a time window.
    /// Used by the partner portal SLA dashboard breach-reasons chart.
    pub async fn breach_reasons(
        &self,
        carrier_id: Uuid,
        from: chrono::DateTime<chrono::Utc>,
        to: chrono::DateTime<chrono::Utc>,
    ) -> AppResult<Vec<crate::domain::repositories::BreachReasonRow>> {
        self.sla_repo
            .breach_reasons(carrier_id, from, to)
            .await
            .map_err(AppError::internal)
    }

    /// Record that the partner submitted compliance documents.
    /// Flips `compliance_status` to `UnderReview` so admin can action them.
    /// In production, the multipart bytes are streamed to object storage by
    /// the HTTP handler before calling this method.
    pub async fn submit_compliance_documents(
        &self,
        id: Uuid,
        _doc_type: &str,
        _filename: &str,
    ) -> AppResult<()> {
        let mut carrier = self.get(id).await?;
        carrier.compliance_status = crate::domain::entities::ComplianceStatus::UnderReview;
        carrier.updated_at = chrono::Utc::now();
        self.repo.save(&carrier).await.map_err(AppError::internal)?;
        Ok(())
    }
}

#[cfg(test)]
mod sla_record_tests {
    use super::*;
    use std::sync::Mutex;
    use chrono::{DateTime, Utc};
    use logisticos_events::{producer::KafkaProducer, topics};
    use rdkafka::{
        config::ClientConfig,
        consumer::{Consumer, StreamConsumer},
    };
    use std::time::Duration;
    use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::TcpListener};

    use crate::infrastructure::messaging::CarrierPublisher;

    #[derive(Default)]
    struct FakeCarriers {
        carriers: Mutex<Vec<Carrier>>,
        fail:     bool,
    }

    #[async_trait::async_trait]
    impl CarrierRepository for FakeCarriers {
        async fn find_by_id(&self, id: &CarrierId) -> anyhow::Result<Option<Carrier>> {
            if self.fail { anyhow::bail!("database unavailable") }
            Ok(self.carriers.lock().unwrap().iter().find(|c| c.id == *id).cloned())
        }
        async fn find_by_code(&self, _: &TenantId, _: &str) -> anyhow::Result<Option<Carrier>> { Ok(None) }
        async fn find_by_contact_email(&self, _: &TenantId, _: &str) -> anyhow::Result<Option<Carrier>> { Ok(None) }
        async fn list(&self, _: &TenantId, _: i64, _: i64) -> anyhow::Result<Vec<Carrier>> { Ok(vec![]) }
        async fn list_active(&self, _: &TenantId) -> anyhow::Result<Vec<Carrier>> { Ok(vec![]) }
        async fn save(&self, _: &Carrier) -> anyhow::Result<()> { Ok(()) }
    }

    #[derive(Default)]
    struct RecordingSla {
        created: Mutex<Vec<SlaRecord>>,
    }

    #[async_trait::async_trait]
    impl SlaRecordRepository for RecordingSla {
        async fn create(&self, r: &SlaRecord) -> anyhow::Result<()> {
            self.created.lock().unwrap().push(r.clone());
            Ok(())
        }
        async fn find_by_shipment(&self, _: Uuid) -> anyhow::Result<Option<SlaRecord>> { Ok(None) }
        async fn save_outcome(&self, _: &SlaRecord) -> anyhow::Result<()> { Ok(()) }
        async fn list_by_carrier(&self, _: Uuid, _: i64, _: i64) -> anyhow::Result<Vec<SlaRecord>> { Ok(vec![]) }
        async fn breach_reasons(&self, _: Uuid, _: DateTime<Utc>, _: DateTime<Utc>) -> anyhow::Result<Vec<crate::domain::repositories::BreachReasonRow>> { Ok(vec![]) }
        async fn zone_summary(&self, _: Uuid, _: DateTime<Utc>, _: DateTime<Utc>) -> anyhow::Result<Vec<ZoneSlaRow>> { Ok(vec![]) }
    }

    struct Harness {
        svc:      CarrierService,
        sla:      Arc<RecordingSla>,
        brokers:  String,
        _cluster: rdkafka::mocking::MockCluster<'static, rdkafka::producer::DefaultProducerContext>,
    }

    /// An in-process broker (the publisher holds a concrete `KafkaProducer`),
    /// whose address lets a test read the allocation topic back.
    fn harness(repo: FakeCarriers) -> Harness {
        let cluster = rdkafka::mocking::MockCluster::new(1).expect("mock kafka cluster");
        let brokers = cluster.bootstrap_servers();
        let producer = Arc::new(KafkaProducer::new(&brokers).expect("producer"));
        let sla = Arc::new(RecordingSla::default());
        let svc = CarrierService::new(
            Arc::new(repo),
            sla.clone(),
            Arc::new(CarrierPublisher::new(producer)),
            Arc::new(crate::infrastructure::external::ExternalCarrierClient::new(None)),
        );
        Harness { svc, sla, brokers, _cluster: cluster }
    }

    fn carrier(tenant: Uuid, endpoint: Option<String>) -> Carrier {
        let mut c = Carrier::new(
            TenantId::from_uuid(tenant),
            "Acme".into(),
            "ACME".into(),
            "ops@acme.test".into(),
            SlaCommitment::default(),
        );
        c.api_endpoint = endpoint;
        c
    }

    fn record(tenant: Uuid, carrier_id: Uuid) -> SlaRecord {
        SlaRecord::new(
            tenant,
            carrier_id,
            Uuid::new_v4(),
            "metro-manila".into(),
            "standard".into(),
            Utc::now() + chrono::Duration::hours(48),
        )
    }

    /// Whether anything is on the allocation topic. The consumer is created
    /// afterwards so it sees the topic as it stands once the call has returned.
    async fn published(h: &Harness, wait: Duration) -> bool {
        // The fixed group.id is safe because every test has its own cluster.
        let consumer: StreamConsumer = ClientConfig::new()
            .set("bootstrap.servers", &h.brokers)
            .set("group.id", "sla-record-tests")
            .set("auto.offset.reset", "earliest")
            .create()
            .expect("consumer");
        consumer.subscribe(&[topics::CARRIER_ALLOCATED]).expect("subscribe");
        tokio::time::timeout(wait, consumer.recv()).await.is_ok_and(|m| m.is_ok())
    }

    /// Answers one HTTP request with 200 so the webhook client sees success.
    async fn serve_one(listener: TcpListener) {
        if let Ok((mut sock, _)) = listener.accept().await {
            let mut buf = [0u8; 4096];
            let _ = sock.read(&mut buf).await;
            let _ = sock.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n").await;
        }
    }

    #[tokio::test]
    async fn sla_record_for_own_carrier_is_created() {
        let tenant = Uuid::new_v4();
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let endpoint = format!("http://{}/hook", listener.local_addr().expect("addr"));
        let own = carrier(tenant, Some(endpoint));
        let rec = record(tenant, own.id.inner());
        let h = harness(FakeCarriers { carriers: Mutex::new(vec![own]), fail: false });
        let server = tokio::spawn(serve_one(listener));

        let saved = h.svc.create_sla_record(rec.clone(), 1_500, "hub_auto".into()).await
            .expect("own carrier is allocated");

        assert_eq!(saved.shipment_id, rec.shipment_id);
        assert_eq!(h.sla.created.lock().unwrap().len(), 1);
        assert!(published(&h, Duration::from_secs(10)).await, "carrier.allocated is published");
        tokio::time::timeout(Duration::from_secs(5), server).await
            .expect("the carrier's webhook is called").expect("server task");
    }

    #[tokio::test]
    async fn sla_record_for_another_tenants_carrier_is_refused_and_writes_nothing() {
        let (tenant_a, tenant_b) = (Uuid::new_v4(), Uuid::new_v4());
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let endpoint = format!("http://{}/hook", listener.local_addr().expect("addr"));
        let foreign = carrier(tenant_b, Some(endpoint));
        let rec = record(tenant_a, foreign.id.inner());
        let h = harness(FakeCarriers { carriers: Mutex::new(vec![foreign]), fail: false });

        let err = h.svc.create_sla_record(rec, 1_500, "hub_auto".into()).await
            .expect_err("another tenant's carrier is refused");

        assert!(matches!(err, AppError::NotFound { .. }), "got {err:?}");
        assert!(h.sla.created.lock().unwrap().is_empty(), "no SLA record inserted");
        assert!(!published(&h, Duration::from_secs(2)).await, "nothing published");
        assert!(
            tokio::time::timeout(Duration::from_millis(500), listener.accept()).await.is_err(),
            "the foreign carrier's webhook was not called"
        );
    }

    #[tokio::test]
    async fn sla_record_for_unknown_carrier_is_refused() {
        let tenant = Uuid::new_v4();
        let h = harness(FakeCarriers::default());

        let err = h.svc.create_sla_record(record(tenant, Uuid::new_v4()), 0, "hub_auto".into()).await
            .expect_err("unknown carrier is refused");

        assert!(matches!(err, AppError::NotFound { .. }), "got {err:?}");
        assert!(h.sla.created.lock().unwrap().is_empty());
        assert!(!published(&h, Duration::from_secs(2)).await, "nothing published");
    }

    #[tokio::test]
    async fn sla_record_is_refused_when_the_carrier_lookup_fails() {
        let tenant = Uuid::new_v4();
        let own = carrier(tenant, None);
        let rec = record(tenant, own.id.inner());
        let h = harness(FakeCarriers { carriers: Mutex::new(vec![own]), fail: true });

        let err = h.svc.create_sla_record(rec, 0, "hub_auto".into()).await
            .expect_err("a lookup failure never falls through to the insert");

        assert!(!matches!(err, AppError::NotFound { .. }), "a DB failure is not a not-found: {err:?}");
        assert!(h.sla.created.lock().unwrap().is_empty());
    }
}
