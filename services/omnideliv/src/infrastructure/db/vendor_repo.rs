use async_trait::async_trait;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::domain::entities::{Vendor, VendorStatus, Vertical};
use crate::domain::repositories::VendorRepository;

pub struct PgVendorRepository { pool: PgPool }

impl PgVendorRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

pub(crate) fn parse_vertical(s: &str) -> anyhow::Result<Vertical> {
    Ok(match s {
        "restaurant" => Vertical::Restaurant,
        "grocery"    => Vertical::Grocery,
        "pharmacy"   => Vertical::Pharmacy,
        "florist"    => Vertical::Florist,
        "retail"     => Vertical::Retail,
        other => anyhow::bail!("unknown vertical in database: {other}"),
    })
}

fn map_row(r: &sqlx::postgres::PgRow) -> anyhow::Result<Vendor> {
    let status_str: String = r.get("status");
    let status = match status_str.as_str() {
        "onboarding" => VendorStatus::Onboarding,
        "active"     => VendorStatus::Active,
        "paused"     => VendorStatus::Paused,
        "offboarded" => VendorStatus::Offboarded,
        other => anyhow::bail!("unknown vendor status in database: {other}"),
    };
    let vertical_str: String = r.get("vertical");

    Ok(Vendor {
        id:                r.get("id"),
        tenant_id:         r.get("tenant_id"),
        vertical:          parse_vertical(&vertical_str)?,
        name:              r.get("name"),
        address:           r.get("address"),
        lat:               r.get("lat"),
        lng:               r.get("lng"),
        prep_time_minutes: r.get("prep_time_minutes"),
        commission_bps:    r.get("commission_bps"),
        payout_account:    r.get("payout_account"),
        hours:             r.get("hours"),
        status,
        created_at:        r.get("created_at"),
        updated_at:        r.get("updated_at"),
    })
}

#[async_trait]
impl VendorRepository for PgVendorRepository {
    async fn find_by_id(&self, tenant_id: Uuid, id: Uuid) -> anyhow::Result<Option<Vendor>> {
        let row = sqlx::query("SELECT * FROM omnideliv.vendors WHERE tenant_id = $1 AND id = $2")
            .bind(tenant_id).bind(id)
            .fetch_optional(&self.pool).await?;
        row.as_ref().map(map_row).transpose()
    }

    async fn save(&self, v: &Vendor) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO omnideliv.vendors (
                id, tenant_id, vertical, name, address, lat, lng,
                prep_time_minutes, commission_bps, payout_account, hours, status,
                created_at, updated_at
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)
            ON CONFLICT (id) DO UPDATE SET
                name              = EXCLUDED.name,
                address           = EXCLUDED.address,
                lat               = EXCLUDED.lat,
                lng               = EXCLUDED.lng,
                prep_time_minutes = EXCLUDED.prep_time_minutes,
                commission_bps    = EXCLUDED.commission_bps,
                payout_account    = EXCLUDED.payout_account,
                hours             = EXCLUDED.hours,
                status            = EXCLUDED.status,
                updated_at        = EXCLUDED.updated_at
            "#,
        )
        .bind(v.id).bind(v.tenant_id).bind(v.vertical.as_str())
        .bind(&v.name).bind(&v.address).bind(v.lat).bind(v.lng)
        .bind(v.prep_time_minutes).bind(v.commission_bps)
        .bind(&v.payout_account).bind(&v.hours).bind(v.status.as_str())
        .bind(v.created_at).bind(v.updated_at)
        .execute(&self.pool).await?;
        Ok(())
    }

    async fn find_near(
        &self,
        tenant_id: Uuid,
        vertical: Vertical,
        lat: f64,
        lng: f64,
        radius_km: f64,
        limit: i64,
    ) -> anyhow::Result<Vec<Vendor>> {
        // Haversine in a subquery so the computed distance is filterable — a
        // WHERE clause cannot reference a SELECT-list alias.
        let rows = sqlx::query(
            r#"
            SELECT * FROM (
                SELECT *,
                       6371 * 2 * ASIN(SQRT(
                           POWER(SIN(RADIANS($4 - lat) / 2), 2) +
                           COS(RADIANS(lat)) * COS(RADIANS($4)) *
                           POWER(SIN(RADIANS($5 - lng) / 2), 2)
                       )) AS distance_km
                FROM omnideliv.vendors
                WHERE tenant_id = $1 AND vertical = $2 AND status = 'active'
            ) AS scored
            WHERE distance_km <= $3
            ORDER BY distance_km ASC
            LIMIT $6
            "#,
        )
        .bind(tenant_id).bind(vertical.as_str()).bind(radius_km)
        .bind(lat).bind(lng).bind(limit)
        .fetch_all(&self.pool).await?;

        rows.iter().map(map_row).collect()
    }
}
