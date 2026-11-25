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
///
/// The crossing time of an episode is `idle_since + idle_minutes`; it is returned
/// when that falls in the half-open interval `(since, as_of]`.
pub fn crossing_bounds(as_of: DateTime<Utc>, since: DateTime<Utc>, idle_minutes: u32) -> (DateTime<Utc>, DateTime<Utc>) {
    let idle = Duration::minutes(i64::from(idle_minutes));
    (as_of - idle, since - idle)
}

/// Lower bound of the crossing interval: the caller's watermark when given,
/// else `as_of - window_secs`. A `since` must satisfy `as_of - 3600s <= since < as_of`.
pub fn resolve_since(
    as_of: DateTime<Utc>,
    window_secs: u32,
    since: Option<DateTime<Utc>>,
) -> Result<DateTime<Utc>, String> {
    match since {
        None => Ok(as_of - Duration::seconds(i64::from(window_secs))),
        Some(s) if s >= as_of => Err("since must be earlier than now".into()),
        Some(s) if s < as_of - Duration::seconds(3600) => Err("since must be within the last 3600s".into()),
        Some(s) => Ok(s),
    }
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
        since: DateTime<Utc>,
        as_of: DateTime<Utc>,
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
        since: DateTime<Utc>,
        as_of: DateTime<Utc>,
    ) -> anyhow::Result<Vec<IdleDriver>> {
        let (newest, oldest) = crossing_bounds(as_of, since, idle_minutes);
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

        if rows.len() as i64 >= MAX_IDLE_ROWS {
            tracing::warn!(%tenant_id, idle_minutes, cap = MAX_IDLE_ROWS, "idle-driver result hit the row cap; some drivers were not returned");
        }

        Ok(rows
            .into_iter()
            .map(|(driver_id, idle_since)| IdleDriver {
                driver_id,
                idle_since,
                idle_minutes: (as_of - idle_since).num_minutes(),
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn t(h: u32, m: u32, sec: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 25, h, m, sec).unwrap()
    }

    #[test]
    fn window_mode_bounds_are_threshold_and_threshold_plus_window() {
        let as_of = t(12, 0, 0);
        let since = resolve_since(as_of, 60, None).unwrap();
        assert_eq!(since, t(11, 59, 0));
        let (newest, oldest) = crossing_bounds(as_of, since, 30);
        assert_eq!(newest, t(11, 30, 0));
        assert_eq!(oldest, t(11, 29, 0));
    }

    #[test]
    fn since_mode_bounds_come_from_the_watermark() {
        let as_of = t(12, 0, 0);
        let since = resolve_since(as_of, 60, Some(t(11, 50, 0))).unwrap();
        assert_eq!(since, t(11, 50, 0));
        let (newest, oldest) = crossing_bounds(as_of, since, 30);
        assert_eq!((newest, oldest), (t(11, 30, 0), t(11, 20, 0)));
    }

    #[test]
    fn consecutive_watermarked_polls_tile_without_gap_or_overlap() {
        // Poll 1 covers (s1, a1]; poll 2 uses since = a1, so (a1, a2]: no gap, no overlap,
        // whatever the jitter between a1 and a2.
        let a1 = t(12, 0, 0);
        let a2 = t(12, 7, 13);
        let (n1, _) = crossing_bounds(a1, t(11, 59, 0), 45);
        let (n2, o2) = crossing_bounds(a2, resolve_since(a2, 60, Some(a1)).unwrap(), 45);
        assert_eq!(o2, n1);
        assert!(n1 < n2);
    }

    #[test]
    fn since_must_be_in_the_past_and_within_an_hour() {
        let as_of = t(12, 0, 0);
        assert!(resolve_since(as_of, 60, Some(as_of)).is_err());
        assert!(resolve_since(as_of, 60, Some(t(12, 0, 1))).is_err());
        assert!(resolve_since(as_of, 60, Some(t(10, 59, 59))).is_err());
        assert!(resolve_since(as_of, 60, Some(t(11, 0, 0))).is_ok());
        assert!(resolve_since(as_of, 60, Some(t(11, 59, 59))).is_ok());
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
