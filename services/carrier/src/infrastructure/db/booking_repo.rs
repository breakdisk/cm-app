//! Append-only repository for carrier.carrier_bookings.
//! Writes one row per successful 3PL API booking (MCP or allocation worker).

use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

pub struct CarrierBookingRecord {
    pub tenant_id:        Uuid,
    pub carrier_code:     String,
    pub shipment_id:      Option<Uuid>,
    pub awb:              Option<String>,
    pub service_code:     String,
    pub booking_ref:      String,
    pub tracking_number:  String,
    pub label_url:        Option<String>,
    pub request_payload:  serde_json::Value,
    pub response_payload: serde_json::Value,
    pub booked_by_actor:  Option<Uuid>,
}

#[derive(Clone)]
pub struct PgCarrierBookingRepository {
    pool: PgPool,
}

impl PgCarrierBookingRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }

    pub async fn insert(&self, rec: &CarrierBookingRecord) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO carrier.carrier_bookings
                (tenant_id, carrier_code, shipment_id, awb, service_code,
                 booking_ref, tracking_number, label_url,
                 request_payload, response_payload, booked_by_actor)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            "#,
        )
        .bind(rec.tenant_id)
        .bind(&rec.carrier_code)
        .bind(rec.shipment_id)
        .bind(&rec.awb)
        .bind(&rec.service_code)
        .bind(&rec.booking_ref)
        .bind(&rec.tracking_number)
        .bind(&rec.label_url)
        .bind(&rec.request_payload)
        .bind(&rec.response_payload)
        .bind(rec.booked_by_actor)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

/// What the MCP tools need from the booking log: write a row, and answer "did
/// THIS tenant book that reference?". Every lookup is scoped by `tenant_id`;
/// a reference another tenant booked is indistinguishable from one that does
/// not exist, so carrier references cannot be probed across tenants.
#[async_trait]
pub trait CarrierBookingStore: Send + Sync {
    async fn insert(&self, rec: &CarrierBookingRecord) -> anyhow::Result<()>;

    /// The carrier code of the tenant's own booking with this `booking_ref`
    /// (optionally restricted to `carrier_code`), or `None`.
    async fn carrier_for_booking_ref(
        &self, tenant_id: Uuid, carrier_code: Option<&str>, booking_ref: &str,
    ) -> anyhow::Result<Option<String>>;

    /// The carrier code of the tenant's own booking with this `tracking_number`.
    async fn carrier_for_tracking_number(
        &self, tenant_id: Uuid, carrier_code: Option<&str>, tracking_number: &str,
    ) -> anyhow::Result<Option<String>>;
}

#[async_trait]
impl CarrierBookingStore for PgCarrierBookingRepository {
    async fn insert(&self, rec: &CarrierBookingRecord) -> anyhow::Result<()> {
        PgCarrierBookingRepository::insert(self, rec).await
    }

    async fn carrier_for_booking_ref(
        &self, tenant_id: Uuid, carrier_code: Option<&str>, booking_ref: &str,
    ) -> anyhow::Result<Option<String>> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT carrier_code FROM carrier.carrier_bookings              WHERE tenant_id = $1 AND booking_ref = $2 AND ($3::text IS NULL OR carrier_code = $3)              ORDER BY created_at DESC LIMIT 1",
        )
        .bind(tenant_id).bind(booking_ref).bind(carrier_code)
        .fetch_optional(&self.pool).await?;
        Ok(row.map(|r| r.0))
    }

    async fn carrier_for_tracking_number(
        &self, tenant_id: Uuid, carrier_code: Option<&str>, tracking_number: &str,
    ) -> anyhow::Result<Option<String>> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT carrier_code FROM carrier.carrier_bookings              WHERE tenant_id = $1 AND tracking_number = $2 AND ($3::text IS NULL OR carrier_code = $3)              ORDER BY created_at DESC LIMIT 1",
        )
        .bind(tenant_id).bind(tracking_number).bind(carrier_code)
        .fetch_optional(&self.pool).await?;
        Ok(row.map(|r| r.0))
    }
}
