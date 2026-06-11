/// Append-only repository for carrier.carrier_bookings.
/// Writes one row per successful 3PL API booking (MCP or allocation worker).

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
