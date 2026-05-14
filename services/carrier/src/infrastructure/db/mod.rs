use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use logisticos_types::TenantId;

use crate::domain::{
    entities::{
        BookingStatus, Carrier, CarrierId, CarrierStatus, ComplianceStatus,
        ListingStatus, MarketplaceBooking, PerformanceGrade, SizeClass,
        SlaCommitment, SlaRecord, SlaStatus, VehicleListing, ZoneSlaRow,
    },
    repositories::{CarrierRepository, MarketplaceRepository, SlaRecordRepository},
};

#[derive(sqlx::FromRow)]
struct CarrierRow {
    id:                Uuid,
    tenant_id:         Uuid,
    name:              String,
    code:              String,
    contact_email:     String,
    contact_phone:     Option<String>,
    api_endpoint:      Option<String>,
    api_key_hash:      Option<String>,
    status:            String,
    compliance_status: String,
    sla:               serde_json::Value,
    rate_cards:        serde_json::Value,
    total_shipments:   i64,
    on_time_count:     i64,
    failed_count:      i64,
    performance_grade: String,
    onboarded_at:      chrono::DateTime<chrono::Utc>,
    updated_at:        chrono::DateTime<chrono::Utc>,
}

impl TryFrom<CarrierRow> for Carrier {
    type Error = anyhow::Error;
    fn try_from(r: CarrierRow) -> Result<Self, Self::Error> {
        Ok(Carrier {
            id:                CarrierId::from_uuid(r.id),
            tenant_id:         TenantId::from_uuid(r.tenant_id),
            name:              r.name,
            code:              r.code,
            contact_email:     r.contact_email,
            contact_phone:     r.contact_phone,
            api_endpoint:      r.api_endpoint,
            has_api_key:       r.api_key_hash.is_some(),
            api_key_hash:      r.api_key_hash,
            status:            serde_json::from_value(serde_json::Value::String(r.status))?,
            compliance_status: serde_json::from_value(serde_json::Value::String(r.compliance_status))
                               .unwrap_or(ComplianceStatus::PendingSubmission),
            sla:               serde_json::from_value(r.sla)?,
            rate_cards:        serde_json::from_value(r.rate_cards)?,
            total_shipments:   r.total_shipments,
            on_time_count:     r.on_time_count,
            failed_count:      r.failed_count,
            performance_grade: serde_json::from_value(serde_json::Value::String(r.performance_grade))?,
            onboarded_at:      r.onboarded_at,
            updated_at:        r.updated_at,
        })
    }
}

pub struct PgCarrierRepository {
    pool: PgPool,
}

impl PgCarrierRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[async_trait]
impl CarrierRepository for PgCarrierRepository {
    async fn find_by_id(&self, id: &CarrierId) -> anyhow::Result<Option<Carrier>> {
        let row = sqlx::query_as::<_, CarrierRow>(
            "SELECT id, tenant_id, name, code, contact_email, contact_phone, api_endpoint, api_key_hash, \
             status, compliance_status, sla, rate_cards, total_shipments, on_time_count, failed_count, performance_grade, \
             onboarded_at, updated_at FROM carrier.carriers WHERE id = $1"
        ).bind(id.inner()).fetch_optional(&self.pool).await?;
        row.map(Carrier::try_from).transpose()
    }

    async fn find_by_code(&self, tenant_id: &TenantId, code: &str) -> anyhow::Result<Option<Carrier>> {
        let row = sqlx::query_as::<_, CarrierRow>(
            "SELECT id, tenant_id, name, code, contact_email, contact_phone, api_endpoint, api_key_hash, \
             status, compliance_status, sla, rate_cards, total_shipments, on_time_count, failed_count, performance_grade, \
             onboarded_at, updated_at FROM carrier.carriers WHERE tenant_id = $1 AND code = $2"
        ).bind(tenant_id.inner()).bind(code).fetch_optional(&self.pool).await?;
        row.map(Carrier::try_from).transpose()
    }

    async fn find_by_contact_email(&self, tenant_id: &TenantId, email: &str) -> anyhow::Result<Option<Carrier>> {
        let row = sqlx::query_as::<_, CarrierRow>(
            "SELECT id, tenant_id, name, code, contact_email, contact_phone, api_endpoint, api_key_hash, \
             status, compliance_status, sla, rate_cards, total_shipments, on_time_count, failed_count, performance_grade, \
             onboarded_at, updated_at FROM carrier.carriers \
             WHERE tenant_id = $1 AND lower(contact_email) = lower($2)"
        ).bind(tenant_id.inner()).bind(email).fetch_optional(&self.pool).await?;
        row.map(Carrier::try_from).transpose()
    }

    async fn list(&self, tenant_id: &TenantId, limit: i64, offset: i64) -> anyhow::Result<Vec<Carrier>> {
        let rows = sqlx::query_as::<_, CarrierRow>(
            "SELECT id, tenant_id, name, code, contact_email, contact_phone, api_endpoint, api_key_hash, \
             status, compliance_status, sla, rate_cards, total_shipments, on_time_count, failed_count, performance_grade, \
             onboarded_at, updated_at FROM carrier.carriers \
             WHERE tenant_id = $1 AND status != 'deactivated' \
             ORDER BY name ASC LIMIT $2 OFFSET $3"
        ).bind(tenant_id.inner()).bind(limit).bind(offset).fetch_all(&self.pool).await?;
        rows.into_iter().map(Carrier::try_from).collect()
    }

    async fn list_active(&self, tenant_id: &TenantId) -> anyhow::Result<Vec<Carrier>> {
        let rows = sqlx::query_as::<_, CarrierRow>(
            "SELECT id, tenant_id, name, code, contact_email, contact_phone, api_endpoint, api_key_hash, \
             status, compliance_status, sla, rate_cards, total_shipments, on_time_count, failed_count, performance_grade, \
             onboarded_at, updated_at FROM carrier.carriers \
             WHERE tenant_id = $1 AND status = 'active'"
        ).bind(tenant_id.inner()).fetch_all(&self.pool).await?;
        rows.into_iter().map(Carrier::try_from).collect()
    }

    async fn save(&self, c: &Carrier) -> anyhow::Result<()> {
        let status            = serde_json::to_value(&c.status)?.as_str().unwrap_or("pending_verification").to_owned();
        let compliance_status = serde_json::to_value(&c.compliance_status)?.as_str().unwrap_or("pending_submission").to_owned();
        let grade             = serde_json::to_value(&c.performance_grade)?.as_str().unwrap_or("good").to_owned();
        let sla               = serde_json::to_value(&c.sla)?;
        let rate_cards        = serde_json::to_value(&c.rate_cards)?;

        sqlx::query(
            r#"
            INSERT INTO carrier.carriers (
                id, tenant_id, name, code, contact_email, contact_phone,
                api_endpoint, api_key_hash, status, compliance_status, sla, rate_cards,
                total_shipments, on_time_count, failed_count, performance_grade,
                onboarded_at, updated_at
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18)
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name, contact_email = EXCLUDED.contact_email,
                contact_phone = EXCLUDED.contact_phone, api_endpoint = EXCLUDED.api_endpoint,
                api_key_hash = EXCLUDED.api_key_hash, status = EXCLUDED.status,
                compliance_status = EXCLUDED.compliance_status,
                sla = EXCLUDED.sla, rate_cards = EXCLUDED.rate_cards,
                total_shipments = EXCLUDED.total_shipments, on_time_count = EXCLUDED.on_time_count,
                failed_count = EXCLUDED.failed_count, performance_grade = EXCLUDED.performance_grade,
                updated_at = EXCLUDED.updated_at
            "#
        )
        .bind(c.id.inner()).bind(c.tenant_id.inner()).bind(&c.name).bind(&c.code).bind(&c.contact_email).bind(&c.contact_phone)
        .bind(&c.api_endpoint).bind(&c.api_key_hash).bind(status).bind(compliance_status).bind(sla).bind(rate_cards)
        .bind(c.total_shipments).bind(c.on_time_count).bind(c.failed_count).bind(grade)
        .bind(c.onboarded_at).bind(c.updated_at)
        .execute(&self.pool).await?;
        Ok(())
    }
}

// ── SLA Record Repository ─────────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct SlaRecordRow {
    id:             Uuid,
    tenant_id:      Uuid,
    carrier_id:     Uuid,
    shipment_id:    Uuid,
    zone:           String,
    service_level:  String,
    promised_by:    DateTime<Utc>,
    delivered_at:   Option<DateTime<Utc>>,
    status:         String,
    on_time:        Option<bool>,
    failure_reason: Option<String>,
    created_at:     DateTime<Utc>,
}

impl From<SlaRecordRow> for SlaRecord {
    fn from(r: SlaRecordRow) -> Self {
        let status = match r.status.as_str() {
            "delivered" => SlaStatus::Delivered,
            "failed"    => SlaStatus::Failed,
            _           => SlaStatus::InTransit,
        };
        SlaRecord {
            id:             r.id,
            tenant_id:      r.tenant_id,
            carrier_id:     r.carrier_id,
            shipment_id:    r.shipment_id,
            zone:           r.zone,
            service_level:  r.service_level,
            promised_by:    r.promised_by,
            delivered_at:   r.delivered_at,
            status,
            on_time:        r.on_time,
            failure_reason: r.failure_reason,
            created_at:     r.created_at,
        }
    }
}

pub struct PgSlaRecordRepository {
    pool: PgPool,
}

impl PgSlaRecordRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[async_trait]
impl SlaRecordRepository for PgSlaRecordRepository {
    async fn create(&self, r: &SlaRecord) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO carrier.sla_records
                (id, tenant_id, carrier_id, shipment_id, zone, service_level,
                 promised_by, status, created_at)
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
            ON CONFLICT (carrier_id, shipment_id) DO NOTHING
            "#
        )
        .bind(r.id).bind(r.tenant_id).bind(r.carrier_id).bind(r.shipment_id)
        .bind(&r.zone).bind(&r.service_level).bind(r.promised_by)
        .bind(r.status.as_str()).bind(r.created_at)
        .execute(&self.pool).await?;
        Ok(())
    }

    async fn find_by_shipment(&self, shipment_id: Uuid) -> anyhow::Result<Option<SlaRecord>> {
        let row = sqlx::query_as::<_, SlaRecordRow>(
            "SELECT id, tenant_id, carrier_id, shipment_id, zone, service_level, \
             promised_by, delivered_at, status, on_time, failure_reason, created_at \
             FROM carrier.sla_records WHERE shipment_id = $1 LIMIT 1"
        ).bind(shipment_id).fetch_optional(&self.pool).await?;
        Ok(row.map(SlaRecord::from))
    }

    async fn save_outcome(&self, r: &SlaRecord) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE carrier.sla_records \
             SET delivered_at = $1, status = $2, on_time = $3, failure_reason = $4 \
             WHERE id = $5"
        )
        .bind(r.delivered_at).bind(r.status.as_str()).bind(r.on_time).bind(&r.failure_reason)
        .bind(r.id)
        .execute(&self.pool).await?;
        Ok(())
    }

    async fn list_by_carrier(&self, carrier_id: Uuid, limit: i64, offset: i64) -> anyhow::Result<Vec<SlaRecord>> {
        let rows = sqlx::query_as::<_, SlaRecordRow>(
            "SELECT id, tenant_id, carrier_id, shipment_id, zone, service_level, \
             promised_by, delivered_at, status, on_time, failure_reason, created_at \
             FROM carrier.sla_records WHERE carrier_id = $1 \
             ORDER BY created_at DESC LIMIT $2 OFFSET $3"
        ).bind(carrier_id).bind(limit).bind(offset).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(SlaRecord::from).collect())
    }

    async fn zone_summary(
        &self,
        carrier_id: Uuid,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> anyhow::Result<Vec<ZoneSlaRow>> {
        #[derive(sqlx::FromRow)]
        struct Row { zone: String, total: i64, on_time_count: i64, failed_count: i64 }

        let rows = sqlx::query_as::<_, Row>(
            r#"
            SELECT
                zone,
                COUNT(*)                                      AS total,
                COUNT(*) FILTER (WHERE on_time = true)        AS on_time_count,
                COUNT(*) FILTER (WHERE on_time = false)       AS failed_count
            FROM carrier.sla_records
            WHERE carrier_id = $1
              AND created_at >= $2
              AND created_at <  $3
              AND status != 'in_transit'
            GROUP BY zone
            ORDER BY total DESC
            "#
        ).bind(carrier_id).bind(from).bind(to).fetch_all(&self.pool).await?;

        Ok(rows.into_iter().map(|r| {
            let on_time_rate = if r.total > 0 {
                r.on_time_count as f64 / r.total as f64 * 100.0
            } else { 0.0 };
            ZoneSlaRow {
                zone:         r.zone,
                total:        r.total,
                on_time:      r.on_time_count,
                failed:       r.failed_count,
                on_time_rate,
            }
        }).collect())
    }
}

// ── Marketplace Repository ────────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct VehicleListingRow {
    id:                           Uuid,
    tenant_id:                    Uuid,
    carrier_id:                   Uuid,
    vehicle_plate:                String,
    size_class:                   String,
    max_weight_kg:                f32,
    max_volume_m3:                Option<f32>,
    base_price_cents:             i64,
    per_km_cents:                 i64,
    per_kg_cents:                 Option<i64>,
    service_area_label:           String,
    idle_from:                    DateTime<Utc>,
    idle_until:                   DateTime<Utc>,
    status:                       String,
    carrier_response_window_mins: i32,
    bookings_today:               i64,
    revenue_today_cents:          i64,
    created_at:                   DateTime<Utc>,
    updated_at:                   DateTime<Utc>,
}

impl TryFrom<VehicleListingRow> for VehicleListing {
    type Error = anyhow::Error;
    fn try_from(r: VehicleListingRow) -> Result<Self, Self::Error> {
        Ok(VehicleListing {
            id:                           r.id,
            tenant_id:                    r.tenant_id,
            carrier_id:                   r.carrier_id,
            vehicle_plate:                r.vehicle_plate,
            size_class:                   SizeClass::from_str(&r.size_class)?,
            max_weight_kg:                r.max_weight_kg,
            max_volume_m3:                r.max_volume_m3,
            base_price_cents:             r.base_price_cents,
            per_km_cents:                 r.per_km_cents,
            per_kg_cents:                 r.per_kg_cents,
            service_area_label:           r.service_area_label,
            idle_from:                    r.idle_from,
            idle_until:                   r.idle_until,
            status:                       ListingStatus::from_str(&r.status)?,
            carrier_response_window_mins: r.carrier_response_window_mins,
            bookings_today:               r.bookings_today,
            revenue_today_cents:          r.revenue_today_cents,
            created_at:                   r.created_at,
            updated_at:                   r.updated_at,
        })
    }
}

#[derive(sqlx::FromRow)]
struct MarketplaceBookingRow {
    id:                 Uuid,
    tenant_id:          Uuid,
    listing_id:         Uuid,
    carrier_id:         Uuid,
    shipment_id:        Uuid,
    awb:                String,
    consumer_name:      String,
    consumer_phone:     Option<String>,
    pickup_label:       String,
    dropoff_label:      String,
    cargo_weight_kg:    f32,
    cargo_volume_m3:    Option<f32>,
    quoted_price_cents: i64,
    status:             String,
    pickup_at:          DateTime<Utc>,
    picked_up_at:       Option<DateTime<Utc>>,
    picked_up_by:       Option<String>,
    pickup_notes:       Option<String>,
    created_at:         DateTime<Utc>,
    updated_at:         DateTime<Utc>,
}

impl TryFrom<MarketplaceBookingRow> for MarketplaceBooking {
    type Error = anyhow::Error;
    fn try_from(r: MarketplaceBookingRow) -> Result<Self, Self::Error> {
        Ok(MarketplaceBooking {
            id:                 r.id,
            tenant_id:          r.tenant_id,
            listing_id:         r.listing_id,
            carrier_id:         r.carrier_id,
            shipment_id:        r.shipment_id,
            awb:                r.awb,
            consumer_name:      r.consumer_name,
            consumer_phone:     r.consumer_phone,
            pickup_label:       r.pickup_label,
            dropoff_label:      r.dropoff_label,
            cargo_weight_kg:    r.cargo_weight_kg,
            cargo_volume_m3:    r.cargo_volume_m3,
            quoted_price_cents: r.quoted_price_cents,
            status:             BookingStatus::from_str(&r.status)?,
            pickup_at:          r.pickup_at,
            picked_up_at:       r.picked_up_at,
            picked_up_by:       r.picked_up_by,
            pickup_notes:       r.pickup_notes,
            created_at:         r.created_at,
            updated_at:         r.updated_at,
        })
    }
}

const LISTING_COLS: &str =
    "id, tenant_id, carrier_id, vehicle_plate, size_class, max_weight_kg, \
     max_volume_m3, base_price_cents, per_km_cents, per_kg_cents, service_area_label, \
     idle_from, idle_until, status, carrier_response_window_mins, \
     bookings_today, revenue_today_cents, created_at, updated_at";

const BOOKING_COLS: &str =
    "id, tenant_id, listing_id, carrier_id, shipment_id, awb, consumer_name, \
     consumer_phone, pickup_label, dropoff_label, cargo_weight_kg, cargo_volume_m3, \
     quoted_price_cents, status, pickup_at, picked_up_at, picked_up_by, \
     pickup_notes, created_at, updated_at";

pub struct PgMarketplaceRepository {
    pool: PgPool,
}

impl PgMarketplaceRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[async_trait]
impl MarketplaceRepository for PgMarketplaceRepository {
    async fn create_listing(&self, l: &VehicleListing) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO carrier.vehicle_listings (
                id, tenant_id, carrier_id, vehicle_plate, size_class,
                max_weight_kg, max_volume_m3, base_price_cents, per_km_cents,
                per_kg_cents, service_area_label, idle_from, idle_until, status,
                carrier_response_window_mins, bookings_today, revenue_today_cents,
                created_at, updated_at
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19)
            "#,
        )
        .bind(l.id).bind(l.tenant_id).bind(l.carrier_id).bind(&l.vehicle_plate)
        .bind(l.size_class.as_str()).bind(l.max_weight_kg).bind(l.max_volume_m3)
        .bind(l.base_price_cents).bind(l.per_km_cents).bind(l.per_kg_cents)
        .bind(&l.service_area_label).bind(l.idle_from).bind(l.idle_until)
        .bind(l.status.as_str()).bind(l.carrier_response_window_mins)
        .bind(l.bookings_today).bind(l.revenue_today_cents)
        .bind(l.created_at).bind(l.updated_at)
        .execute(&self.pool).await?;
        Ok(())
    }

    async fn find_listing_by_id(&self, id: Uuid) -> anyhow::Result<Option<VehicleListing>> {
        let row = sqlx::query_as::<_, VehicleListingRow>(
            &format!("SELECT {LISTING_COLS} FROM carrier.vehicle_listings WHERE id = $1"),
        ).bind(id).fetch_optional(&self.pool).await?;
        row.map(VehicleListing::try_from).transpose()
    }

    async fn list_listings_by_carrier(
        &self, carrier_id: Uuid, limit: i64, offset: i64,
    ) -> anyhow::Result<Vec<VehicleListing>> {
        let rows = sqlx::query_as::<_, VehicleListingRow>(
            &format!(
                "SELECT {LISTING_COLS} FROM carrier.vehicle_listings \
                 WHERE carrier_id = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3"
            ),
        ).bind(carrier_id).bind(limit).bind(offset).fetch_all(&self.pool).await?;
        rows.into_iter().map(VehicleListing::try_from).collect()
    }

    async fn update_listing(&self, l: &VehicleListing) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE carrier.vehicle_listings SET \
             vehicle_plate = $1, size_class = $2, max_weight_kg = $3, max_volume_m3 = $4, \
             base_price_cents = $5, per_km_cents = $6, per_kg_cents = $7, \
             service_area_label = $8, idle_from = $9, idle_until = $10, status = $11, \
             carrier_response_window_mins = $12, bookings_today = $13, \
             revenue_today_cents = $14, updated_at = $15 \
             WHERE id = $16",
        )
        .bind(&l.vehicle_plate).bind(l.size_class.as_str()).bind(l.max_weight_kg)
        .bind(l.max_volume_m3).bind(l.base_price_cents).bind(l.per_km_cents)
        .bind(l.per_kg_cents).bind(&l.service_area_label).bind(l.idle_from)
        .bind(l.idle_until).bind(l.status.as_str()).bind(l.carrier_response_window_mins)
        .bind(l.bookings_today).bind(l.revenue_today_cents).bind(l.updated_at)
        .bind(l.id)
        .execute(&self.pool).await?;
        Ok(())
    }

    async fn delete_listing(&self, id: Uuid) -> anyhow::Result<bool> {
        let result = sqlx::query("DELETE FROM carrier.vehicle_listings WHERE id = $1")
            .bind(id)
            .execute(&self.pool).await?;
        Ok(result.rows_affected() > 0)
    }

    async fn create_booking(&self, b: &MarketplaceBooking) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO carrier.marketplace_bookings (
                id, tenant_id, listing_id, carrier_id, shipment_id, awb,
                consumer_name, consumer_phone, pickup_label, dropoff_label,
                cargo_weight_kg, cargo_volume_m3, quoted_price_cents, status,
                pickup_at, picked_up_at, picked_up_by, pickup_notes, created_at, updated_at
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20)
            "#,
        )
        .bind(b.id).bind(b.tenant_id).bind(b.listing_id).bind(b.carrier_id)
        .bind(b.shipment_id).bind(&b.awb).bind(&b.consumer_name).bind(&b.consumer_phone)
        .bind(&b.pickup_label).bind(&b.dropoff_label).bind(b.cargo_weight_kg)
        .bind(b.cargo_volume_m3).bind(b.quoted_price_cents).bind(b.status.as_str())
        .bind(b.pickup_at).bind(b.picked_up_at).bind(&b.picked_up_by)
        .bind(&b.pickup_notes).bind(b.created_at).bind(b.updated_at)
        .execute(&self.pool).await?;
        Ok(())
    }

    async fn find_booking_by_id(&self, id: Uuid) -> anyhow::Result<Option<MarketplaceBooking>> {
        let row = sqlx::query_as::<_, MarketplaceBookingRow>(
            &format!("SELECT {BOOKING_COLS} FROM carrier.marketplace_bookings WHERE id = $1"),
        ).bind(id).fetch_optional(&self.pool).await?;
        row.map(MarketplaceBooking::try_from).transpose()
    }

    async fn list_bookings_by_carrier(
        &self, carrier_id: Uuid, limit: i64, offset: i64,
    ) -> anyhow::Result<Vec<MarketplaceBooking>> {
        let rows = sqlx::query_as::<_, MarketplaceBookingRow>(
            &format!(
                "SELECT {BOOKING_COLS} FROM carrier.marketplace_bookings \
                 WHERE carrier_id = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3"
            ),
        ).bind(carrier_id).bind(limit).bind(offset).fetch_all(&self.pool).await?;
        rows.into_iter().map(MarketplaceBooking::try_from).collect()
    }

    async fn save_booking(&self, b: &MarketplaceBooking) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE carrier.marketplace_bookings SET \
             status = $1, picked_up_at = $2, picked_up_by = $3, pickup_notes = $4, \
             updated_at = $5 WHERE id = $6",
        )
        .bind(b.status.as_str()).bind(b.picked_up_at).bind(&b.picked_up_by)
        .bind(&b.pickup_notes).bind(b.updated_at).bind(b.id)
        .execute(&self.pool).await?;
        Ok(())
    }
}
