use async_trait::async_trait;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::domain::entities::{Courier, CourierStatus};
use crate::domain::repositories::CourierRepository;

pub struct PgCourierRepository {
    pool: PgPool,
}

impl PgCourierRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

fn map_row(r: &sqlx::postgres::PgRow) -> anyhow::Result<Courier> {
    let status_str: String = r.get("status");
    let status = match status_str.as_str() {
        "offline"   => CourierStatus::Offline,
        "available" => CourierStatus::Available,
        "assigned"  => CourierStatus::Assigned,
        "on_break"  => CourierStatus::OnBreak,
        other => anyhow::bail!("unknown courier status in database: {other}"),
    };

    Ok(Courier {
        id:           r.get("id"),
        tenant_id:    r.get("tenant_id"),
        user_id:      r.get("user_id"),
        first_name:   r.get("first_name"),
        last_name:    r.get("last_name"),
        phone:        r.get("phone"),
        status,
        vehicle_type: r.get("vehicle_type"),
        zone:         r.get("zone"),
        last_lat:     r.get("last_lat"),
        last_lng:     r.get("last_lng"),
        last_seen_at: r.get("last_seen_at"),
        is_active:    r.get("is_active"),
        created_at:   r.get("created_at"),
        updated_at:   r.get("updated_at"),
    })
}

#[async_trait]
impl CourierRepository for PgCourierRepository {
    async fn find_by_id(&self, tenant_id: Uuid, id: Uuid) -> anyhow::Result<Option<Courier>> {
        let row = sqlx::query(
            "SELECT * FROM field_ops.couriers WHERE tenant_id = $1 AND id = $2",
        )
        .bind(tenant_id)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        row.as_ref().map(map_row).transpose()
    }

    async fn find_by_user(&self, tenant_id: Uuid, user_id: Uuid) -> anyhow::Result<Option<Courier>> {
        let row = sqlx::query(
            "SELECT * FROM field_ops.couriers WHERE tenant_id = $1 AND user_id = $2",
        )
        .bind(tenant_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;

        row.as_ref().map(map_row).transpose()
    }

    async fn save(&self, c: &Courier) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO field_ops.couriers (
                id, tenant_id, user_id, first_name, last_name, phone, status,
                vehicle_type, zone, last_lat, last_lng, last_seen_at, is_active,
                created_at, updated_at
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)
            ON CONFLICT (id) DO UPDATE SET
                first_name   = EXCLUDED.first_name,
                last_name    = EXCLUDED.last_name,
                phone        = EXCLUDED.phone,
                status       = EXCLUDED.status,
                vehicle_type = EXCLUDED.vehicle_type,
                zone         = EXCLUDED.zone,
                last_lat     = EXCLUDED.last_lat,
                last_lng     = EXCLUDED.last_lng,
                last_seen_at = EXCLUDED.last_seen_at,
                is_active    = EXCLUDED.is_active,
                updated_at   = EXCLUDED.updated_at
            "#,
        )
        .bind(c.id).bind(c.tenant_id).bind(c.user_id)
        .bind(&c.first_name).bind(&c.last_name).bind(&c.phone)
        // `CourierStatus::as_str` rather than a second mapping function here —
        // the entity owns the database representation, and two copies of this
        // match is exactly how `on_break` becomes `onbreak` in one of them.
        .bind(c.status.as_str())
        .bind(&c.vehicle_type).bind(&c.zone)
        .bind(c.last_lat).bind(c.last_lng).bind(c.last_seen_at)
        .bind(c.is_active).bind(c.created_at).bind(c.updated_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn list_for_tenant(
        &self,
        tenant_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> anyhow::Result<Vec<Courier>> {
        // No status filter and no join to the location table. This is the ops
        // roster: it must show the suspended, the offline and the never-seen,
        // which is exactly what `find_available_near` is built to exclude.
        let rows = sqlx::query(
            r#"
            SELECT * FROM field_ops.couriers
             WHERE tenant_id = $1
             ORDER BY created_at DESC
             LIMIT $2 OFFSET $3
            "#,
        )
        .bind(tenant_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        rows.iter().map(map_row).collect()
    }

    async fn find_available_near(
        &self,
        tenant_id: Uuid,
        lat: f64,
        lng: f64,
        radius_km: f64,
        limit: i64,
    ) -> anyhow::Result<Vec<Courier>> {
        // PostGIS ST_DWithin against field_ops.courier_latest_locations, which
        // is what the GiST index in migration 0003 serves. This mirrors
        // dispatch's driver_avail_repo query against driver_latest_locations,
        // deliberately: convergence should be a repository swap, and two
        // different notions of "nearest available field worker" would make it
        // a reconciliation instead.
        //
        // Not Haversine over couriers.last_lat/last_lng — those are a render
        // cache. Arithmetic on them cannot use an index at all, so every supply
        // lookup would scan the courier table.
        //
        // INNER JOIN, not LEFT: a courier with no fix has no position to search
        // on. driver_ops LEFT JOINs and sorts no-fix drivers last, which is
        // right for "show me the fleet" and wrong for "who can take this job".
        let rows = sqlx::query(
            r#"
            SELECT c.*,
                   ST_Distance(
                       geography(ST_SetSRID(ST_MakePoint(cl.lng, cl.lat), 4326)),
                       ST_SetSRID(ST_MakePoint($4, $3), 4326)::geography
                   ) AS distance_m
              FROM field_ops.couriers c
              JOIN field_ops.courier_latest_locations cl
                ON cl.courier_id = c.id
               AND cl.recorded_at > NOW() - INTERVAL '10 minutes'
             WHERE c.tenant_id = $1
               AND c.is_active
               AND c.status = 'available'
               -- A courier already holding a live claim cannot take another
               -- job: `uq_courier_single_live_claim` (migration 0002) is a
               -- partial unique index on (courier_id) WHERE status='claimed',
               -- so their claim would be rejected by the database.
               --
               -- Without this the two disagree. `couriers.status` is a
               -- denormalised field that has to be written back, and when that
               -- write is missed the courier reads as available forever: they
               -- keep winning proximity searches, keep being offered work, and
               -- can only ever answer `{"won":false}`. The order then waits out
               -- the recovery window and escalates, with nothing in the logs
               -- pointing at the cause. Observed live on 2026-08-07.
               --
               -- The index is the authority on "has a live job" because it is
               -- what actually enforces it; asking it directly means there is
               -- no second copy of that fact to drift.
               AND NOT EXISTS (
                   SELECT 1
                     FROM field_ops.courier_assignments a
                    WHERE a.courier_id = c.id
                      AND a.status = 'claimed'
               )
               AND ST_DWithin(
                       geography(ST_SetSRID(ST_MakePoint(cl.lng, cl.lat), 4326)),
                       ST_SetSRID(ST_MakePoint($4, $3), 4326)::geography,
                       $2 * 1000.0
                   )
             ORDER BY distance_m ASC
             LIMIT $5
            "#,
        )
        .bind(tenant_id)
        .bind(radius_km)
        .bind(lat)
        .bind(lng)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        rows.iter().map(map_row).collect()
    }
}
