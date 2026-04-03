use async_trait::async_trait;
use sqlx::PgPool;
use logisticos_types::{DriverId, Address, Coordinates};
use uuid::Uuid;
use crate::domain::{
    entities::{DriverTask, TaskStatus, TaskType},
    repositories::TaskRepository,
};

pub struct PgTaskRepository {
    pool: PgPool,
}

impl PgTaskRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[derive(sqlx::FromRow)]
struct TaskRow {
    id:                   Uuid,
    driver_id:            Uuid,
    route_id:             Uuid,
    shipment_id:          Uuid,
    task_type:            String,
    sequence:             i32,
    status:               String,
    address_line1:        String,
    address_line2:        Option<String>,
    city:                 String,
    province:             String,
    postal_code:          String,
    country_code:         String,
    lat:                  Option<f64>,
    lng:                  Option<f64>,
    customer_name:        String,
    customer_phone:       String,
    cod_amount_cents:     Option<i64>,
    special_instructions: Option<String>,
    pod_id:               Option<Uuid>,
    started_at:           Option<chrono::DateTime<chrono::Utc>>,
    completed_at:         Option<chrono::DateTime<chrono::Utc>>,
    failed_reason:        Option<String>,
}

fn parse_task_status(s: &str) -> TaskStatus {
    match s {
        "in_progress" => TaskStatus::InProgress,
        "completed"   => TaskStatus::Completed,
        "failed"      => TaskStatus::Failed,
        "skipped"     => TaskStatus::Skipped,
        _             => TaskStatus::Pending,
    }
}

fn status_str(s: TaskStatus) -> &'static str {
    match s {
        TaskStatus::Pending    => "pending",
        TaskStatus::InProgress => "in_progress",
        TaskStatus::Completed  => "completed",
        TaskStatus::Failed     => "failed",
        TaskStatus::Skipped    => "skipped",
    }
}

impl From<TaskRow> for DriverTask {
    fn from(r: TaskRow) -> Self {
        DriverTask {
            id: r.id,
            driver_id: DriverId::from_uuid(r.driver_id),
            route_id: r.route_id,
            shipment_id: r.shipment_id,
            task_type: if r.task_type == "pickup" { TaskType::Pickup } else { TaskType::Delivery },
            sequence: r.sequence,
            status: parse_task_status(&r.status),
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
            customer_name: r.customer_name,
            customer_phone: r.customer_phone,
            cod_amount_cents: r.cod_amount_cents,
            special_instructions: r.special_instructions,
            pod_id: r.pod_id,
            started_at: r.started_at,
            completed_at: r.completed_at,
            failed_reason: r.failed_reason,
        }
    }
}

#[async_trait]
impl TaskRepository for PgTaskRepository {
    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<DriverTask>> {
        let row = sqlx::query_as::<_, TaskRow>(
            r#"SELECT id, driver_id, route_id, shipment_id, task_type, sequence, status,
                      address_line1, address_line2, city, province, postal_code, country AS country_code,
                      lat, lng, customer_name, customer_phone, cod_amount_cents,
                      special_instructions, pod_id, started_at, completed_at, failed_reason
               FROM driver_ops.tasks WHERE id = $1"#
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(DriverTask::from))
    }

    async fn list_by_driver(&self, driver_id: &DriverId) -> anyhow::Result<Vec<DriverTask>> {
        let rows = sqlx::query_as::<_, TaskRow>(
            r#"SELECT id, driver_id, route_id, shipment_id, task_type, sequence, status,
                      address_line1, address_line2, city, province, postal_code, country AS country_code,
                      lat, lng, customer_name, customer_phone, cod_amount_cents,
                      special_instructions, pod_id, started_at, completed_at, failed_reason
               FROM driver_ops.tasks
               WHERE driver_id = $1
               ORDER BY sequence ASC"#
        )
        .bind(driver_id.inner())
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(DriverTask::from).collect())
    }

    async fn list_by_route(&self, route_id: Uuid) -> anyhow::Result<Vec<DriverTask>> {
        let rows = sqlx::query_as::<_, TaskRow>(
            r#"SELECT id, driver_id, route_id, shipment_id, task_type, sequence, status,
                      address_line1, address_line2, city, province, postal_code, country AS country_code,
                      lat, lng, customer_name, customer_phone, cod_amount_cents,
                      special_instructions, pod_id, started_at, completed_at, failed_reason
               FROM driver_ops.tasks
               WHERE route_id = $1
               ORDER BY sequence ASC"#
        )
        .bind(route_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(DriverTask::from).collect())
    }

    async fn save(&self, t: &DriverTask) -> anyhow::Result<()> {
        let status = status_str(t.status);
        let task_type = if matches!(t.task_type, TaskType::Pickup) { "pickup" } else { "delivery" };
        sqlx::query(
            r#"INSERT INTO driver_ops.tasks
                   (id, driver_id, route_id, shipment_id, task_type, sequence, status,
                    address_line1, address_line2, city, province, postal_code, country,
                    lat, lng, customer_name, customer_phone, cod_amount_cents,
                    special_instructions, pod_id, started_at, completed_at, failed_reason)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23)
               ON CONFLICT (id) DO UPDATE SET
                   status        = EXCLUDED.status,
                   pod_id        = EXCLUDED.pod_id,
                   started_at    = EXCLUDED.started_at,
                   completed_at  = EXCLUDED.completed_at,
                   failed_reason = EXCLUDED.failed_reason"#
        )
        .bind(t.id)
        .bind(t.driver_id.inner())
        .bind(t.route_id)
        .bind(t.shipment_id)
        .bind(task_type)
        .bind(t.sequence)
        .bind(status)
        .bind(&t.address.line1)
        .bind(&t.address.line2)
        .bind(&t.address.city)
        .bind(&t.address.province)
        .bind(&t.address.postal_code)
        .bind(&t.address.country_code)
        .bind(t.address.coordinates.map(|c| c.lat))
        .bind(t.address.coordinates.map(|c| c.lng))
        .bind(&t.customer_name)
        .bind(&t.customer_phone)
        .bind(t.cod_amount_cents)
        .bind(&t.special_instructions)
        .bind(t.pod_id)
        .bind(t.started_at)
        .bind(t.completed_at)
        .bind(&t.failed_reason)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn bulk_save(&self, tasks: &[DriverTask]) -> anyhow::Result<()> {
        for task in tasks {
            self.save(task).await?;
        }
        Ok(())
    }
}
