use async_trait::async_trait;
use sqlx::PgPool;
use logisticos_types::{Coordinates, DriverId, TenantId};
use crate::domain::repositories::{DriverAvailabilityRepository, AvailableDriver};

/// Reads from the `driver_ops.drivers` and `driver_ops.driver_locations` tables
/// (cross-schema read — driver-ops schema is visible in the same PostgreSQL instance).
/// In a true microservices deployment this would call the driver-ops API or read
/// from an event-replicated materialized view.
pub struct PgDriverAvailabilityRepository {
    pool: PgPool,
}

impl PgDriverAvailabilityRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[derive(sqlx::FromRow)]
struct AvailableDriverRow {
    driver_id:         uuid::Uuid,
    first_name:        String,
    last_name:         String,
    lat:               f64,
    lng:               f64,
    distance_meters:   f64,
    active_stop_count: i64,
}

#[async_trait]
impl DriverAvailabilityRepository for PgDriverAvailabilityRepository {
    async fn find_available_near(
        &self,
        tenant_id: &TenantId,
        coords: Coordinates,
        radius_km: f64,
    ) -> anyhow::Result<Vec<AvailableDriver>> {
        // Uses PostGIS ST_DWithin for spatial proximity filtering.
        // Joins with dispatch.driver_assignments to exclude drivers with active routes.
        // stop_count comes from dispatch.route_stops for loaded-driver awareness.
        let rows = sqlx::query_as::<_, AvailableDriverRow>(
            r#"
            SELECT
                d.id                    AS driver_id,
                d.first_name,
                d.last_name,
                ST_Y(dl.location::geometry) AS lat,
                ST_X(dl.location::geometry) AS lng,
                ST_Distance(
                    dl.location,
                    ST_SetSRID(ST_MakePoint($2, $3), 4326)::geography
                ) AS distance_meters,
                COALESCE(stop_counts.cnt, 0) AS active_stop_count
            FROM driver_ops.drivers d
            INNER JOIN driver_ops.driver_locations dl ON dl.driver_id = d.id
            LEFT JOIN (
                SELECT da.driver_id, COUNT(rs.id) AS cnt
                FROM dispatch.driver_assignments da
                JOIN dispatch.routes r ON r.id = da.route_id
                JOIN dispatch.route_stops rs ON rs.route_id = r.id
                WHERE da.status IN ('pending', 'accepted')
                  AND r.status = 'in_progress'
                GROUP BY da.driver_id
            ) stop_counts ON stop_counts.driver_id = d.id
            WHERE d.tenant_id = $1
              AND d.is_active = true
              AND d.status = 'online'
              AND ST_DWithin(
                  dl.location,
                  ST_SetSRID(ST_MakePoint($2, $3), 4326)::geography,
                  $4
              )
              -- Exclude drivers already assigned to an active route
              AND NOT EXISTS (
                  SELECT 1 FROM dispatch.driver_assignments da2
                  WHERE da2.driver_id = d.id
                    AND da2.status IN ('pending', 'accepted')
              )
              -- Only use fresh location data (< 10 minutes old)
              AND dl.recorded_at > NOW() - INTERVAL '10 minutes'
            ORDER BY distance_meters ASC
            "#
        )
        .bind(tenant_id.inner())
        .bind(coords.lng)  // ST_MakePoint(lng, lat) — PostGIS convention
        .bind(coords.lat)
        .bind(radius_km * 1000.0)  // Convert km to meters for ST_DWithin geography
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| AvailableDriver {
            driver_id: DriverId::from_uuid(r.driver_id),
            name: format!("{} {}", r.first_name, r.last_name),
            distance_km: r.distance_meters / 1000.0,
            location: Coordinates { lat: r.lat, lng: r.lng },
            active_stop_count: r.active_stop_count as u32,
        }).collect())
    }
}
