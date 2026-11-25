//! Idle-driver query for business-logic's `DriverIdleExceeded` poller.
//!
//! Idle is the absence of events, so no Kafka message ever says "this driver has
//! been idle for N minutes". The caller polls with the rule's own threshold and
//! this query returns only the drivers that *crossed* it since the previous poll.

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::entities::TaskStatus;

pub const MAX_IDLE_ROWS: i64 = 1000;

/// One driver that crossed the idle threshold inside the polled window.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct IdleDriver {
    /// The driver's `user_id` (`drivers.id = user_id`, enforced by migration 0014).
    pub driver_id:    Uuid,
    pub idle_since:   DateTime<Utc>,
    pub idle_minutes: i64,
}

/// `(newest, oldest)` bounds for `idle_since`: a driver is returned when
/// `oldest < idle_since <= newest`. `newest = now - idle_minutes` (the threshold
/// has been reached) and `oldest = newest - window_secs` (it had not been reached
/// at the previous poll), so each idle episode lands in exactly one poll.
pub fn crossing_window(now: DateTime<Utc>, idle_minutes: u32, window_secs: u32) -> (DateTime<Utc>, DateTime<Utc>) {
    let newest = now - Duration::minutes(i64::from(idle_minutes));
    let oldest = newest - Duration::seconds(i64::from(window_secs));
    (newest, oldest)
}

/// Task statuses that mean the driver is still working. Every other status is
/// terminal. Exhaustive: a new `TaskStatus` variant will not compile until it is
/// classified here.
pub fn is_active_task_status(s: TaskStatus) -> bool {
    match s {
        TaskStatus::Pending | TaskStatus::InProgress => true,
        TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Skipped | TaskStatus::Cancelled => false,
    }
}

/// DB spellings of the active statuses (`tasks.status` CHECK values).
pub const ACTIVE_TASK_STATUSES: [&str; 2] = ["pending", "in_progress"];

#[async_trait]
pub trait IdleDriverStore: Send + Sync {
    async fn idle_crossing(
        &self,
        tenant_id: Uuid,
        idle_minutes: u32,
        window_secs: u32,
        now: DateTime<Utc>,
    ) -> anyhow::Result<Vec<IdleDriver>>;
}

pub struct PgIdleDriverStore {
    pool: PgPool,
}

impl PgIdleDriverStore {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[async_trait]
impl IdleDriverStore for PgIdleDriverStore {
    async fn idle_crossing(
        &self,
        tenant_id: Uuid,
        idle_minutes: u32,
        window_secs: u32,
        now: DateTime<Utc>,
    ) -> anyhow::Result<Vec<IdleDriver>> {
        let (newest, oldest) = crossing_window(now, idle_minutes, window_secs);
        let active: Vec<String> = ACTIVE_TASK_STATUSES.iter().map(|s| (*s).to_owned()).collect();

        // `tasks` carries no tenant_id; tenant scope comes from `drivers`.
        // GREATEST ignores NULLs in Postgres, so a driver with no completed task
        // is idle since their duty session opened.
        let rows: Vec<(Uuid, DateTime<Utc>)> = sqlx::query_as(
            r#"
            SELECT driver_id, idle_since FROM (
                SELECT d.id AS driver_id,
                       GREATEST(s.started_at, (
                           SELECT MAX(t.completed_at) FROM driver_ops.tasks t WHERE t.driver_id = d.id
                       )) AS idle_since
                  FROM driver_ops.drivers d
                  JOIN driver_ops.duty_sessions s
                    ON s.driver_id = d.id AND s.tenant_id = d.tenant_id AND s.ended_at IS NULL
                 WHERE d.tenant_id = $1
                   AND d.status = 'available'
                   AND NOT EXISTS (
                       SELECT 1 FROM driver_ops.tasks a
                        WHERE a.driver_id = d.id AND a.status = ANY($4)
                   )
            ) x
            WHERE idle_since <= $2 AND idle_since > $3
            ORDER BY idle_since
            LIMIT $5
            "#,
        )
        .bind(tenant_id)
        .bind(newest)
        .bind(oldest)
        .bind(&active)
        .bind(MAX_IDLE_ROWS)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(driver_id, idle_since)| IdleDriver {
                driver_id,
                idle_since,
                idle_minutes: (now - idle_since).num_minutes(),
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn window_bounds_are_threshold_and_threshold_plus_window() {
        let now = Utc.with_ymd_and_hms(2026, 11, 25, 12, 0, 0).unwrap();
        let (newest, oldest) = crossing_window(now, 30, 60);
        assert_eq!(newest, Utc.with_ymd_and_hms(2026, 11, 25, 11, 30, 0).unwrap());
        assert_eq!(oldest, Utc.with_ymd_and_hms(2026, 11, 25, 11, 29, 0).unwrap());
    }

    #[test]
    fn consecutive_polls_tile_without_gap_or_overlap() {
        // Poll 1 covers (o1, n1], poll 2 (exactly one window later) covers (o2, n2].
        // A driver is in one poll only if o2 == n1.
        let t0 = Utc.with_ymd_and_hms(2026, 11, 25, 12, 0, 0).unwrap();
        let (n1, o1) = crossing_window(t0, 45, 60);
        let (n2, o2) = crossing_window(t0 + Duration::seconds(60), 45, 60);
        assert_eq!(o2, n1);
        assert!(o1 < n1 && n1 < n2);
    }

    #[test]
    fn active_statuses_match_the_db_strings() {
        use TaskStatus::*;
        let all = [Pending, InProgress, Completed, Failed, Skipped, Cancelled];
        let active: Vec<_> = all.into_iter().filter(|s| is_active_task_status(*s)).collect();
        assert_eq!(active, vec![Pending, InProgress]);
        assert_eq!(ACTIVE_TASK_STATUSES, ["pending", "in_progress"]);
    }
}
