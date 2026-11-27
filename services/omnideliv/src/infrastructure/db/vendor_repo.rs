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
        user_id:           r.get("user_id"),
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
        slug:              r.get("slug"),
        custom_domain:     r.get("custom_domain"),
        tagline:           r.get("tagline"),
        public_enabled:    r.get("public_enabled"),
        created_at:        r.get("created_at"),
        updated_at:        r.get("updated_at"),
    })
}

#[async_trait]
impl VendorRepository for PgVendorRepository {
    async fn find_public_storefront(&self, handle: &str) -> anyhow::Result<Option<Vendor>> {
        // No tenant predicate -- see the note on the trait. `public_enabled` is
        // in the WHERE rather than checked afterwards, so an unpublished
        // storefront reads as absent instead of as "exists but hidden", which
        // is the difference between a 404 and a confirmation that a vendor is
        // on this platform.
        //
        // One query for both keys: a Host header and a slug are the same kind
        // of public handle, and splitting them would mean the caller has to
        // know which it holds.
        let h = handle.trim().to_ascii_lowercase();
        let row = sqlx::query(
            r#"
            SELECT * FROM omnideliv.vendors
             WHERE public_enabled = TRUE
               AND (slug = $1 OR lower(custom_domain) = $1)
             LIMIT 1
            "#,
        )
        .bind(&h)
        .fetch_optional(&self.pool)
        .await?;

        row.as_ref().map(map_row).transpose()
    }

    async fn set_public_handle(
        &self,
        tenant_id:      Uuid,
        vendor_id:      Uuid,
        slug:           Option<&str>,
        custom_domain:  Option<&str>,
        tagline:        Option<&str>,
        public_enabled: bool,
    ) -> anyhow::Result<bool> {
        let n = sqlx::query(
            r#"
            UPDATE omnideliv.vendors
               SET slug = $3, custom_domain = $4, tagline = $5,
                   public_enabled = $6, updated_at = NOW()
             WHERE id = $1 AND tenant_id = $2
            "#,
        )
        .bind(vendor_id)
        .bind(tenant_id)
        .bind(slug)
        .bind(custom_domain)
        .bind(tagline)
        .bind(public_enabled)
        .execute(&self.pool)
        .await?
        .rows_affected();
        Ok(n == 1)
    }

    async fn find_by_id(&self, tenant_id: Uuid, id: Uuid) -> anyhow::Result<Option<Vendor>> {
        let row = sqlx::query("SELECT * FROM omnideliv.vendors WHERE tenant_id = $1 AND id = $2")
            .bind(tenant_id).bind(id)
            .fetch_optional(&self.pool).await?;
        row.as_ref().map(map_row).transpose()
    }

    async fn find_by_user(&self, tenant_id: Uuid, user_id: Uuid) -> anyhow::Result<Option<Vendor>> {
        let row = sqlx::query(
            "SELECT * FROM omnideliv.vendors WHERE tenant_id = $1 AND user_id = $2",
        )
        .bind(tenant_id).bind(user_id)
        .fetch_optional(&self.pool).await?;
        row.as_ref().map(map_row).transpose()
    }

    async fn list_for_tenant(&self, tenant_id: Uuid) -> anyhow::Result<Vec<Vendor>> {
        let rows = sqlx::query(
            "SELECT * FROM omnideliv.vendors WHERE tenant_id = $1 ORDER BY created_at DESC"
        )
        .bind(tenant_id)
        .fetch_all(&self.pool).await?;
        rows.iter().map(map_row).collect()
    }

    async fn save(&self, v: &Vendor) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO omnideliv.vendors (
                id, tenant_id, user_id, vertical, name, address, lat, lng,
                prep_time_minutes, commission_bps, payout_account, hours, status,
                created_at, updated_at
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)
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
                user_id           = EXCLUDED.user_id,
                updated_at        = EXCLUDED.updated_at
            "#,
        )
        .bind(v.id).bind(v.tenant_id).bind(v.user_id).bind(v.vertical.as_str())
        .bind(&v.name).bind(&v.address).bind(v.lat).bind(v.lng)
        .bind(v.prep_time_minutes).bind(v.commission_bps)
        .bind(&v.payout_account).bind(&v.hours).bind(v.status.as_str())
        .bind(v.created_at).bind(v.updated_at)
        .execute(&self.pool).await?;
        Ok(())
    }

    async fn find_by_ids(&self, tenant_id: Uuid, ids: &[Uuid]) -> anyhow::Result<Vec<Vendor>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        // `= ANY($2)` rather than a built IN-list: one prepared statement
        // regardless of how many stops an order has, and no string building
        // anywhere near a query.
        let rows = sqlx::query(
            "SELECT * FROM omnideliv.vendors WHERE tenant_id = $1 AND id = ANY($2)",
        )
        .bind(tenant_id)
        .bind(ids)
        .fetch_all(&self.pool)
        .await?;

        rows.iter().map(map_row).collect()
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
