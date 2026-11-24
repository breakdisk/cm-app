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
        "completed" => AssignmentStatus::Completed,
        "pending"   => AssignmentStatus::Pending,
        // Unknown must never read as active: Pending would pin a driver as
        // busy forever. Fail closed to a terminal state and make noise.
        other => {
            tracing::error!(status = other, "unknown driver_assignments.status; treating as cancelled");
            AssignmentStatus::Cancelled
        }
    }
}

fn status_str(s: AssignmentStatus) -> &'static str {
    match s {
        AssignmentStatus::Pending   => "pending",
        AssignmentStatus::Accepted  => "accepted",
        AssignmentStatus::Rejected  => "rejected",
        AssignmentStatus::Cancelled => "cancelled",
        AssignmentStatus::Completed => "completed",
    }
}

impl From<AssignmentRow> for DriverAssignment {
    fn from(r: AssignmentRow) -> Self {
        DriverAssignment {
            id: r.id,
            tenant_id: TenantId::from_uuid(r.tenant_id),
            driver_id: DriverId::from_uuid(r.driver_id),
            route_id: RouteId::from_uuid(r.route_id),
            status: parse_status(&r.status),
            assigned_at: r.assigned_at,
            accepted_at: r.accepted_at,
            rejected_at: r.rejected_at,
            rejection_reason: r.rejection_reason,
        }
    }
}

/// "The work on this route has ended": at least one task, none open, and
/// every expected task exists. "Expected" is the leg count dispatch recorded
/// on the assignment when it emitted the legs (`expected_tasks`); a legacy row
/// without one falls back to the route's stop count. Quick-dispatch and
/// gig-claim routes have no route stops, so without `expected_tasks` a
/// finished pickup would close the assignment before the delivery leg arrived.
/// Only `pending`/`in_progress` are open; every other task status counts as
/// finished.
fn finished_predicate(route: &str) -> String {
    format!(
        "EXISTS (SELECT 1 FROM driver_ops.tasks t WHERE t.route_id = {route})
         AND NOT EXISTS (SELECT 1 FROM driver_ops.tasks t
                          WHERE t.route_id = {route} AND t.status IN ('pending','in_progress'))
         AND (SELECT COUNT(*) FROM driver_ops.tasks t WHERE t.route_id = {route})
             >= COALESCE(
                  (SELECT x.expected_tasks FROM dispatch.driver_assignments x
                    WHERE x.route_id = {route}
                    ORDER BY x.assigned_at DESC, x.id DESC LIMIT 1),
                  (SELECT COUNT(*) FROM dispatch.route_stops s WHERE s.route_id = {route}))"
    )
}

#[async_trait]
impl DriverAssignmentRepository for PgDriverAssignmentRepository {
    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<DriverAssignment>> {
        let row = sqlx::query_as::<_, AssignmentRow>(
            r#"SELECT id, tenant_id, driver_id, route_id, status,
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
            r#"SELECT id, tenant_id, driver_id, route_id, status,
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

    async fn cancel_active_for_route(&self, tenant_id: Uuid, route_id: Uuid) -> anyhow::Result<bool> {
        let result = sqlx::query(
            r#"UPDATE dispatch.driver_assignments a
               SET status = 'cancelled'
               WHERE a.route_id = $1
                 AND a.tenant_id = $2
                 AND a.status IN ('pending', 'accepted')
                 AND EXISTS (SELECT 1 FROM dispatch.routes r
                              WHERE r.id = a.route_id AND r.status = 'cancelled')"#,
        )
        .bind(route_id)
        .bind(tenant_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn find_route_for_outcome(
        &self,
        tenant_id: Uuid,
        task_id: Option<Uuid>,
        shipment_id: Option<Uuid>,
    ) -> anyhow::Result<Option<Uuid>> {
        if let Some(task_id) = task_id {
            let by_task: Option<Uuid> = sqlx::query_scalar(
                r#"SELECT r.id FROM driver_ops.tasks t
                     JOIN dispatch.routes r ON r.id = t.route_id
                    WHERE t.id = $1 AND r.tenant_id = $2"#,
            )
            .bind(task_id)
            .bind(tenant_id)
            .fetch_optional(&self.pool)
            .await?;
            if by_task.is_some() {
                return Ok(by_task);
            }
        }
        let Some(shipment_id) = shipment_id else { return Ok(None) };
        let by_shipment: Option<Uuid> = sqlx::query_scalar(
            r#"SELECT r.id FROM dispatch.routes r
                WHERE r.tenant_id = $2
                  AND (EXISTS (SELECT 1 FROM dispatch.route_stops s
                                WHERE s.route_id = r.id AND s.shipment_id = $1)
                       OR EXISTS (SELECT 1 FROM driver_ops.tasks t
                                   WHERE t.route_id = r.id AND t.shipment_id = $1))
                  AND (r.status IN ('planned', 'in_progress')
                       OR EXISTS (SELECT 1 FROM dispatch.driver_assignments a
                                   WHERE a.route_id = r.id AND a.status IN ('pending', 'accepted')))
                ORDER BY r.created_at DESC
                LIMIT 1"#,
        )
        .bind(shipment_id)
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(by_shipment)
    }

    async fn close_if_finished(&self, tenant_id: Uuid, route_id: Uuid) -> anyhow::Result<bool> {
        // One statement = one transaction. Two replicas racing on the same
        // route both pass the predicate on their snapshot, but each UPDATE
        // re-checks its status filter after waiting on the row lock, so the
        // loser matches nothing. A replay matches nothing either.
        let sql = format!(
            r#"WITH done AS (SELECT 1 AS ok WHERE {pred}),
                    closed AS (
                        UPDATE dispatch.driver_assignments a
                           SET status = 'completed'
                         WHERE a.route_id = $1 AND a.tenant_id = $2
                           AND a.status IN ('pending', 'accepted')
                           AND EXISTS (SELECT 1 FROM done)
                        RETURNING a.id),
                    rt AS (
                        UPDATE dispatch.routes r
                           SET status = 'completed', completed_at = NOW(), updated_at = NOW()
                         WHERE r.id = $1 AND r.tenant_id = $2
                           AND r.status IN ('planned', 'in_progress')
                           AND EXISTS (SELECT 1 FROM done)
                        RETURNING r.id)
               SELECT (SELECT COUNT(*) FROM closed) + (SELECT COUNT(*) FROM rt)"#,
            pred = finished_predicate("$1::uuid"),
        );
        let changed: i64 = sqlx::query_scalar(&sql)
            .bind(route_id)
            .bind(tenant_id)
            .fetch_one(&self.pool)
            .await?;
        Ok(changed > 0)
    }

    async fn close_finished_assignments(&self, limit: i64) -> anyhow::Result<u64> {
        let sql = format!(
            r#"SELECT DISTINCT a.route_id, a.tenant_id
                 FROM dispatch.driver_assignments a
                WHERE a.status = 'accepted' AND {pred}
                LIMIT $1"#,
            pred = finished_predicate("a.route_id"),
        );
        let candidates: Vec<(Uuid, Uuid)> = sqlx::query_as(&sql)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;
        let mut closed = 0u64;
        for (route_id, tenant_id) in candidates {
            if self.close_if_finished(tenant_id, route_id).await? {
                closed += 1;
            }
        }
        Ok(closed)
    }

    async fn set_expected_tasks(&self, assignment_id: Uuid, expected: i32) -> anyhow::Result<()> {
        sqlx::query("UPDATE dispatch.driver_assignments SET expected_tasks = $2 WHERE id = $1")
            .bind(assignment_id)
            .bind(expected)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn save(&self, a: &DriverAssignment) -> anyhow::Result<()> {
        let status = status_str(a.status);
        sqlx::query(
            r#"INSERT INTO dispatch.driver_assignments
                   (id, tenant_id, driver_id, route_id, status,
                    assigned_at, accepted_at, rejected_at, rejection_reason)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
               ON CONFLICT (id) DO UPDATE SET
                   status           = EXCLUDED.status,
                   accepted_at      = EXCLUDED.accepted_at,
                   rejected_at      = EXCLUDED.rejected_at,
                   rejection_reason = EXCLUDED.rejection_reason"#
        )
        .bind(a.id)
        .bind(a.tenant_id.inner())
        .bind(a.driver_id.inner())
        .bind(a.route_id.inner())
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

#[cfg(test)]
mod assignment_status_tests {
    use super::*;

    #[test]
    fn completed_round_trips() {
        assert_eq!(status_str(AssignmentStatus::Completed), "completed");
        assert_eq!(parse_status("completed"), AssignmentStatus::Completed);
    }

    #[test]
    fn unknown_status_is_not_silently_active() {
        // An unrecognised value must never read as an active (Pending) assignment:
        // that would hold a driver "busy" forever on a value this image cannot read.
        let parsed = parse_status("some_future_status");
        assert_eq!(parsed, AssignmentStatus::Cancelled);
        assert!(!DriverAssignment {
            id: Uuid::nil(),
            tenant_id: TenantId::from_uuid(Uuid::nil()),
            driver_id: DriverId::from_uuid(Uuid::nil()),
            route_id: RouteId::from_uuid(Uuid::nil()),
            status: parsed,
            assigned_at: chrono::Utc::now(),
            accepted_at: None,
            rejected_at: None,
            rejection_reason: None,
        }
        .is_active());
    }
}
