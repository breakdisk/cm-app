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
    country:              String,
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
            sequence: r.sequence as u32,
            status: parse_task_status(&r.status),
            address: Address {
                line1: r.address_line1,
                line2: r.address_line2,
                city: r.city,
                province: r.province,
                postal_code: r.postal_code,
                country: r.country,
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
        let row = sqlx::query_as!(
            TaskRow,
            r#"SELECT id, driver_id, route_id, shipment_id, task_type, sequence, status,
                      address_line1, address_line2, city, province, postal_code, country,
                      lat, lng, customer_name, customer_phone, cod_amount_cents,
                      special_instructions, pod_id, started_at, completed_at, failed_reason
               FROM driver_ops.tasks WHERE id = $1"#,
            id
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(DriverTask::from))
    }

    async fn list_by_driver(&self, driver_id: &DriverId) -> anyhow::Result<Vec<DriverTask>> {
        let rows = sqlx::query_as!(
            TaskRow,
            r#"SELECT id, driver_id, route_id, shipment_id, task_type, sequence, status,
                      address_line1, address_line2, city, province, postal_code, country,
                      lat, lng, customer_name, customer_phone, cod_amount_cents,
                      special_instructions, pod_id, started_at, completed_at, failed_reason
               FROM driver_ops.tasks
               WHERE driver_id = $1
               ORDER BY sequence ASC"#,
            driver_id.inner()
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(DriverTask::from).collect())
    }

    async fn list_by_route(&self, route_id: Uuid) -> anyhow::Result<Vec<DriverTask>> {
        let rows = sqlx::query_as!(
            TaskRow,
            r#"SELECT id, driver_id, route_id, shipment_id, task_type, sequence, status,
                      address_line1, address_line2, city, province, postal_code, country,
                      lat, lng, customer_name, customer_phone, cod_amount_cents,
                      special_instructions, pod_id, started_at, completed_at, failed_reason
               FROM driver_ops.tasks
               WHERE route_id = $1
               ORDER BY sequence ASC"#,
            route_id
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(DriverTask::from).collect())
    }

    async fn save(&self, t: &DriverTask) -> anyhow::Result<()> {
        let status = status_str(t.status);
        let task_type = if matches!(t.task_type, TaskType::Pickup) { "pickup" } else { "delivery" };
        sqlx::query!(
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
                   failed_reason = EXCLUDED.failed_reason"#,
            t.id, t.driver_id.inner(), t.route_id, t.shipment_id,
            task_type, t.sequence as i32, status,
            t.address.line1, t.address.line2,
            t.address.city, t.address.province, t.address.postal_code, t.address.country,
            t.address.coordinates.map(|c| c.lat),
            t.address.coordinates.map(|c| c.lng),
            t.customer_name, t.customer_phone,
            t.cod_amount_cents,
            t.special_instructions,
            t.pod_id, t.started_at, t.completed_at, t.failed_reason,
        )
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
