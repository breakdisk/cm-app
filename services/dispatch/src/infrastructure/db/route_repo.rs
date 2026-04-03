use async_trait::async_trait;
use sqlx::PgPool;
use logisticos_types::{DriverId, RouteId, TenantId, VehicleId, Address, Coordinates};
use crate::domain::{
    entities::{Route, DeliveryStop, StopType, RouteStatus},
    repositories::RouteRepository,
};

pub struct PgRouteRepository {
    pool: PgPool,
}

impl PgRouteRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[derive(sqlx::FromRow)]
struct RouteRow {
    id:                        uuid::Uuid,
    tenant_id:                 uuid::Uuid,
    driver_id:                 uuid::Uuid,
    vehicle_id:                uuid::Uuid,
    status:                    String,
    total_distance_km:         f64,
    estimated_duration_minutes: i32,
    created_at:                chrono::DateTime<chrono::Utc>,
    started_at:                Option<chrono::DateTime<chrono::Utc>>,
    completed_at:              Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(sqlx::FromRow)]
struct StopRow {
    sequence:             i32,
    shipment_id:          uuid::Uuid,
    address_line1:        String,
    address_line2:        Option<String>,
    city:                 String,
    province:             String,
    postal_code:          String,
    country_code:         String,
    lat:                  Option<f64>,
    lng:                  Option<f64>,
    stop_type:            String,
    time_window_start:    Option<chrono::DateTime<chrono::Utc>>,
    time_window_end:      Option<chrono::DateTime<chrono::Utc>>,
    estimated_arrival:    Option<chrono::DateTime<chrono::Utc>>,
    actual_arrival:       Option<chrono::DateTime<chrono::Utc>>,
}

fn parse_status(s: &str) -> RouteStatus {
    match s {
        "in_progress" => RouteStatus::InProgress,
        "completed"   => RouteStatus::Completed,
        "cancelled"   => RouteStatus::Cancelled,
        _             => RouteStatus::Planned,
    }
}

fn status_str(s: RouteStatus) -> &'static str {
    match s {
        RouteStatus::Planned    => "planned",
        RouteStatus::InProgress => "in_progress",
        RouteStatus::Completed  => "completed",
        RouteStatus::Cancelled  => "cancelled",
    }
}

impl From<StopRow> for DeliveryStop {
    fn from(r: StopRow) -> Self {
        DeliveryStop {
            sequence: r.sequence as u32,
            shipment_id: r.shipment_id,
            address: Address {
                line1: r.address_line1,
                line2: r.address_line2,
                city: r.city,
                province: r.province,
                postal_code: r.postal_code,
                country_code: r.country_code,
                barangay: None,
                coordinates: match (r.lat, r.lng) {
                    (Some(lat), Some(lng)) => Some(Coordinates { lat, lng }),
                    _ => None,
                },
            },
            stop_type: match r.stop_type.as_str() {
                "pickup" => StopType::Pickup,
                _ => StopType::Delivery,
            },
            time_window_start: r.time_window_start,
            time_window_end: r.time_window_end,
            estimated_arrival: r.estimated_arrival,
            actual_arrival: r.actual_arrival,
        }
    }
}

#[async_trait]
impl RouteRepository for PgRouteRepository {
    async fn find_by_id(&self, id: &RouteId) -> anyhow::Result<Option<Route>> {
        let row = sqlx::query_as::<_, RouteRow>(
            r#"SELECT id, tenant_id, driver_id, vehicle_id, status,
                      total_distance_km, estimated_duration_minutes,
                      created_at, started_at, completed_at
               FROM dispatch.routes WHERE id = $1"#
        )
        .bind(id.inner())
        .fetch_optional(&self.pool)
        .await?;

        let Some(row) = row else { return Ok(None) };
        let stops = self.load_stops(row.id).await?;
        Ok(Some(route_from_row(row, stops)))
    }

    async fn find_active_by_driver(&self, driver_id: &DriverId) -> anyhow::Result<Option<Route>> {
        let row = sqlx::query_as::<_, RouteRow>(
            r#"SELECT id, tenant_id, driver_id, vehicle_id, status,
                      total_distance_km, estimated_duration_minutes,
                      created_at, started_at, completed_at
               FROM dispatch.routes
               WHERE driver_id = $1 AND status IN ('planned', 'in_progress')
               ORDER BY created_at DESC
               LIMIT 1"#
        )
        .bind(driver_id.inner())
        .fetch_optional(&self.pool)
        .await?;

        let Some(row) = row else { return Ok(None) };
        let stops = self.load_stops(row.id).await?;
        Ok(Some(route_from_row(row, stops)))
    }

    async fn list_by_tenant(&self, tenant_id: &TenantId) -> anyhow::Result<Vec<Route>> {
        let rows = sqlx::query_as::<_, RouteRow>(
            r#"SELECT id, tenant_id, driver_id, vehicle_id, status,
                      total_distance_km, estimated_duration_minutes,
                      created_at, started_at, completed_at
               FROM dispatch.routes
               WHERE tenant_id = $1
               ORDER BY created_at DESC"#
        )
        .bind(tenant_id.inner())
        .fetch_all(&self.pool)
        .await?;

        let mut routes = Vec::with_capacity(rows.len());
        for row in rows {
            let stops = self.load_stops(row.id).await?;
            routes.push(route_from_row(row, stops));
        }
        Ok(routes)
    }

    async fn save(&self, route: &Route) -> anyhow::Result<()> {
        let status = status_str(route.status);
        sqlx::query(
            r#"INSERT INTO dispatch.routes
                   (id, tenant_id, driver_id, vehicle_id, status,
                    total_distance_km, estimated_duration_minutes,
                    created_at, started_at, completed_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
               ON CONFLICT (id) DO UPDATE SET
                   status                    = EXCLUDED.status,
                   total_distance_km         = EXCLUDED.total_distance_km,
                   estimated_duration_minutes = EXCLUDED.estimated_duration_minutes,
                   started_at               = EXCLUDED.started_at,
                   completed_at             = EXCLUDED.completed_at"#
        )
        .bind(route.id.inner())
        .bind(route.tenant_id.inner())
        .bind(route.driver_id.inner())
        .bind(route.vehicle_id.inner())
        .bind(status)
        .bind(route.total_distance_km)
        .bind(route.estimated_duration_minutes as i32)
        .bind(route.created_at)
        .bind(route.started_at)
        .bind(route.completed_at)
        .execute(&self.pool)
        .await?;

        // Upsert stops — delete-and-reinsert for simplicity (stops rarely change after creation)
        sqlx::query("DELETE FROM dispatch.route_stops WHERE route_id = $1")
            .bind(route.id.inner())
            .execute(&self.pool)
            .await?;

        for stop in &route.stops {
            let stop_type = match stop.stop_type {
                StopType::Pickup   => "pickup",
                StopType::Delivery => "delivery",
            };
            sqlx::query(
                r#"INSERT INTO dispatch.route_stops
                       (route_id, sequence, shipment_id,
                        address_line1, address_line2, city, province, postal_code, country_code,
                        lat, lng, stop_type,
                        time_window_start, time_window_end, estimated_arrival, actual_arrival)
                   VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)"#
            )
            .bind(route.id.inner())
            .bind(stop.sequence as i32)
            .bind(stop.shipment_id)
            .bind(&stop.address.line1)
            .bind(&stop.address.line2)
            .bind(&stop.address.city)
            .bind(&stop.address.province)
            .bind(&stop.address.postal_code)
            .bind(&stop.address.country_code)
            .bind(stop.address.coordinates.map(|c| c.lat))
            .bind(stop.address.coordinates.map(|c| c.lng))
            .bind(stop_type)
            .bind(stop.time_window_start)
            .bind(stop.time_window_end)
            .bind(stop.estimated_arrival)
            .bind(stop.actual_arrival)
            .execute(&self.pool)
            .await?;
        }

        Ok(())
    }
}

impl PgRouteRepository {
    async fn load_stops(&self, route_id: uuid::Uuid) -> anyhow::Result<Vec<DeliveryStop>> {
        let rows = sqlx::query_as::<_, StopRow>(
            r#"SELECT sequence, shipment_id,
                      address_line1, address_line2, city, province, postal_code, country,
                      lat, lng, stop_type,
                      time_window_start, time_window_end, estimated_arrival, actual_arrival
               FROM dispatch.route_stops
               WHERE route_id = $1
               ORDER BY sequence ASC"#
        )
        .bind(route_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(DeliveryStop::from).collect())
    }
}

fn route_from_row(row: RouteRow, stops: Vec<DeliveryStop>) -> Route {
    Route {
        id: RouteId::from_uuid(row.id),
        tenant_id: TenantId::from_uuid(row.tenant_id),
        driver_id: DriverId::from_uuid(row.driver_id),
        vehicle_id: VehicleId::from_uuid(row.vehicle_id),
        stops,
        status: parse_status(&row.status),
        total_distance_km: row.total_distance_km,
        estimated_duration_minutes: row.estimated_duration_minutes as u32,
        created_at: row.created_at,
        started_at: row.started_at,
        completed_at: row.completed_at,
    }
}
