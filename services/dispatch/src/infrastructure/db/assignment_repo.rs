use async_trait::async_trait;
use sqlx::PgPool;
use logisticos_types::{DriverId, RouteId, TenantId};
use uuid::Uuid;
use crate::domain::{
    entities::{DriverAssignment, AssignmentStatus},
    repositories::DriverAssignmentRepository,
};

pub struct PgDriverAssignmentRepository {
    pool: PgPool,
}

impl PgDriverAssignmentRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[derive(sqlx::FromRow)]
struct AssignmentRow {
    id:               Uuid,
    tenant_id:        Uuid,
    driver_id:        Uuid,
    route_id:         Uuid,
    shipment_id:      Option<Uuid>,
    status:           String,
    assigned_at:      chrono::DateTime<chrono::Utc>,
    accepted_at:      Option<chrono::DateTime<chrono::Utc>>,
    rejected_at:      Option<chrono::DateTime<chrono::Utc>>,
    rejection_reason: Option<String>,
}

fn parse_status(s: &str) -> AssignmentStatus {
    match s {
        "accepted"  => AssignmentStatus::Accepted,
        "rejected"  => AssignmentStatus::Rejected,
        "cancelled" => AssignmentStatus::Cancelled,
        _           => AssignmentStatus::Pending,
    }
}

fn status_str(s: AssignmentStatus) -> &'static str {
    match s {
        AssignmentStatus::Pending   => "pending",
        AssignmentStatus::Accepted  => "accepted",
        AssignmentStatus::Rejected  => "rejected",
        AssignmentStatus::Cancelled => "cancelled",
    }
}

impl From<AssignmentRow> for DriverAssignment {
    fn from(r: AssignmentRow) -> Self {
        DriverAssignment {
            id: r.id,
            tenant_id: TenantId::from_uuid(r.tenant_id),
            driver_id: DriverId::from_uuid(r.driver_id),
            route_id: RouteId::from_uuid(r.route_id),
            shipment_id: r.shipment_id,
            status: parse_status(&r.status),
            assigned_at: r.assigned_at,
            accepted_at: r.accepted_at,
            rejected_at: r.rejected_at,
            rejection_reason: r.rejection_reason,
        }
    }
}

#[async_trait]
impl DriverAssignmentRepository for PgDriverAssignmentRepository {
    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<DriverAssignment>> {
        let row = sqlx::query_as::<_, AssignmentRow>(
            r#"SELECT id, tenant_id, driver_id, route_id, shipment_id, status,
                      assigned_at, accepted_at, rejected_at, rejection_reason
               FROM dispatch.driver_assignments WHERE id = $1"#
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(DriverAssignment::from))
    }

    async fn find_active_by_driver(&self, driver_id: &DriverId) -> anyhow::Result<Option<DriverAssignment>> {
        let row = sqlx::query_as::<_, AssignmentRow>(
            r#"SELECT id, tenant_id, driver_id, route_id, shipment_id, status,
                      assigned_at, accepted_at, rejected_at, rejection_reason
               FROM dispatch.driver_assignments
               WHERE driver_id = $1 AND status IN ('pending', 'accepted')
               ORDER BY assigned_at DESC
               LIMIT 1"#
        )
        .bind(driver_id.inner())
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(DriverAssignment::from))
    }

    async fn cancel_active_for_driver(&self, driver_id: &DriverId) -> anyhow::Result<bool> {
        let result = sqlx::query(
            r#"UPDATE dispatch.driver_assignments
               SET status = 'cancelled'
               WHERE driver_id = $1
                 AND status IN ('pending', 'accepted')"#,
        )
        .bind(driver_id.inner())
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn save(&self, a: &DriverAssignment) -> anyhow::Result<()> {
        let status = status_str(a.status);
        sqlx::query(
            r#"INSERT INTO dispatch.driver_assignments
                   (id, tenant_id, driver_id, route_id, shipment_id, status,
                    assigned_at, accepted_at, rejected_at, rejection_reason)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
               ON CONFLICT (id) DO UPDATE SET
                   shipment_id      = EXCLUDED.shipment_id,
                   status           = EXCLUDED.status,
                   accepted_at      = EXCLUDED.accepted_at,
                   rejected_at      = EXCLUDED.rejected_at,
                   rejection_reason = EXCLUDED.rejection_reason"#
        )
        .bind(a.id)
        .bind(a.tenant_id.inner())
        .bind(a.driver_id.inner())
        .bind(a.route_id.inner())
        .bind(a.shipment_id)
        .bind(status)
        .bind(a.assigned_at)
        .bind(a.accepted_at)
        .bind(a.rejected_at)
        .bind(&a.rejection_reason)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
