//! Whole-home moves in dispatch: what each needs, which leads can take it,
//! and the reservation a lead's claim makes.

use chrono::{DateTime, Datelike, NaiveDate, Utc};
use logisticos_events::payloads::HomeMoveRequirement;
use serde::Serialize;
use sqlx::{PgPool, Row};
use uuid::Uuid;

pub struct HomeRepo {
    pool: PgPool,
}

/// A lead who can take the move, with where they last were.
#[derive(Debug, Clone, PartialEq)]
pub struct HomeLead {
    pub driver_id: Uuid,
    pub lat: Option<f64>,
    pub lng: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReserveOutcome {
    Reserved { tenant_id: Uuid, shipment_id: Uuid, move_date: NaiveDate, all_candidate_ids: Vec<Uuid> },
    NotACandidate,
    AlreadyTaken,
    Expired,
    /// The lead's jobs for that day are already full.
    DayFull,
}

/// One of a lead's reserved home moves.
#[derive(Debug, Clone, Serialize)]
pub struct LeadMove {
    pub shipment_id: Uuid,
    pub tracking_number: Option<String>,
    pub customer_name: String,
    pub pickup: String,
    pub dropoff: String,
    pub move_at: DateTime<Utc>,
    pub move_date: NaiveDate,
    pub survey_at: Option<DateTime<Utc>>,
    pub trucks: i32,
    pub helpers: i32,
    pub crew_total: i32,
    pub large_estate: bool,
    pub international: bool,
    /// Turned into the day's assignment already.
    pub activated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReservationView {
    pub shipment_id: Uuid,
    pub driver_id: Uuid,
    pub lead_name: String,
    pub move_date: NaiveDate,
    pub reserved_at: DateTime<Utc>,
    pub activated_at: Option<DateTime<Utc>>,
}

/// The move's date when the event predates it: the UTC date of the move.
pub fn move_date_of(req: &HomeMoveRequirement) -> NaiveDate {
    req.move_date.unwrap_or_else(|| req.move_at.date_naive())
}

/// 0 = Monday, as provider working days count.
pub fn weekday_of(date: NaiveDate) -> i16 {
    i16::try_from(date.weekday().num_days_from_monday()).unwrap_or(0)
}

/// A multi-truck job needs a multi-truck lead with enough trucks.
pub fn needs_multi_truck(req: &HomeMoveRequirement) -> bool {
    req.trucks > 1 || req.large_estate
}

impl HomeRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn save_requirement(&self, tenant_id: Uuid, shipment_id: Uuid, req: &HomeMoveRequirement) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO dispatch.home_requirements
                    (shipment_id, tenant_id, trucks, helpers, crew_total, large_estate, international,
                     move_at, move_date, survey_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
             ON CONFLICT (shipment_id) DO NOTHING",
        )
        .bind(shipment_id)
        .bind(tenant_id)
        .bind(i32::try_from(req.trucks).unwrap_or(i32::MAX))
        .bind(i32::try_from(req.helpers).unwrap_or(i32::MAX))
        .bind(i32::try_from(req.crew_total).unwrap_or(i32::MAX))
        .bind(req.large_estate)
        .bind(req.international)
        .bind(req.move_at)
        .bind(move_date_of(req))
        .bind(req.survey_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn requirement(&self, shipment_id: Uuid) -> anyhow::Result<Option<HomeMoveRequirement>> {
        let row = sqlx::query(
            "SELECT trucks, helpers, crew_total, large_estate, international, move_at, move_date, survey_at
               FROM dispatch.home_requirements WHERE shipment_id = $1",
        )
        .bind(shipment_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| HomeMoveRequirement {
            trucks: i64::from(r.get::<i32, _>("trucks")),
            helpers: i64::from(r.get::<i32, _>("helpers")),
            crew_total: i64::from(r.get::<i32, _>("crew_total")),
            large_estate: r.get("large_estate"),
            international: r.get("international"),
            move_at: r.get("move_at"),
            move_date: Some(r.get("move_date")),
            survey_at: r.get("survey_at"),
        }))
    }

    pub async fn is_reserved(&self, shipment_id: Uuid) -> anyhow::Result<bool> {
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM dispatch.home_reservations WHERE shipment_id = $1 AND released_at IS NULL",
        )
        .bind(shipment_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(n > 0)
    }

    /// Every home-move lead in the tenant who can meet this move: coverage,
    /// trucks, helpers, working that day, not off, and not already full.
    pub async fn eligible_leads(&self, tenant_id: Uuid, req: &HomeMoveRequirement) -> anyhow::Result<Vec<HomeLead>> {
        let date = move_date_of(req);
        let rows = sqlx::query(
            "SELECT p.driver_id, d.lat, d.lng
               FROM driver_ops.provider_profiles p
               JOIN driver_ops.drivers d ON d.id = p.driver_id AND d.is_active
              WHERE p.tenant_id = $1
                AND 'home_move' = ANY(p.service_lines)
                AND (CASE WHEN $2 THEN 'international' ELSE 'local' END) = ANY(p.coverage)
                AND (NOT $3 OR (p.multi_truck_capable AND p.fleet_trucks >= $4))
                AND p.registered_helpers >= $5
                AND $6::smallint = ANY(p.working_days)
                AND NOT EXISTS (SELECT 1 FROM driver_ops.provider_off_days o
                                 WHERE o.driver_id = p.driver_id AND o.day = $7)
                AND (SELECT COUNT(*) FROM dispatch.home_reservations r
                      WHERE r.driver_id = p.driver_id AND r.move_date = $7 AND r.released_at IS NULL)
                    < p.max_daily_jobs",
        )
        .bind(tenant_id)
        .bind(req.international)
        .bind(needs_multi_truck(req))
        .bind(i32::try_from(req.trucks).unwrap_or(i32::MAX))
        .bind(i32::try_from(req.helpers).unwrap_or(i32::MAX))
        .bind(weekday_of(date))
        .bind(date)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| HomeLead { driver_id: r.get("driver_id"), lat: r.get("lat"), lng: r.get("lng") })
            .collect())
    }

    /// Whether this offer is for a home move.
    pub async fn offer_is_home(&self, offer_id: Uuid) -> anyhow::Result<bool> {
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM dispatch.task_offers o
               JOIN dispatch.home_requirements h ON h.shipment_id = o.shipment_id
              WHERE o.id = $1",
        )
        .bind(offer_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(n > 0)
    }

    /// The lead's claim on a home offer: the offer closes, the lead is
    /// reserved for the day, the queue row goes back to pending for the
    /// activation sweep. The lead's profile row is locked so two claims for
    /// the same day cannot both pass the jobs-a-day check.
    pub async fn reserve(&self, offer_id: Uuid, driver_id: Uuid) -> anyhow::Result<ReserveOutcome> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET LOCAL lock_timeout = '250ms'").execute(&mut *tx).await?;

        let candidate: Option<(Uuid,)> =
            sqlx::query_as("SELECT driver_id FROM dispatch.task_offer_candidates WHERE offer_id = $1 AND driver_id = $2")
                .bind(offer_id)
                .bind(driver_id)
                .fetch_optional(&mut *tx)
                .await?;
        if candidate.is_none() {
            return Ok(ReserveOutcome::NotACandidate);
        }

        let won = sqlx::query(
            "UPDATE dispatch.task_offers SET status = 'claimed', claimed_by = $2, claimed_at = NOW()
              WHERE id = $1 AND status = 'open' AND expires_at > NOW()
              RETURNING tenant_id, shipment_id",
        )
        .bind(offer_id)
        .bind(driver_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = won else {
            let status: Option<(String,)> = sqlx::query_as("SELECT status FROM dispatch.task_offers WHERE id = $1")
                .bind(offer_id)
                .fetch_optional(&mut *tx)
                .await?;
            return Ok(match status {
                Some((s,)) if s == "claimed" => ReserveOutcome::AlreadyTaken,
                Some(_) => ReserveOutcome::Expired,
                None => ReserveOutcome::NotACandidate,
            });
        };
        let tenant_id: Uuid = row.get("tenant_id");
        let shipment_id: Uuid = row.get("shipment_id");

        let max_jobs: Option<(i32,)> =
            sqlx::query_as("SELECT max_daily_jobs FROM driver_ops.provider_profiles WHERE driver_id = $1 FOR UPDATE")
                .bind(driver_id)
                .fetch_optional(&mut *tx)
                .await?;
        let move_date: NaiveDate =
            sqlx::query_scalar("SELECT move_date FROM dispatch.home_requirements WHERE shipment_id = $1")
                .bind(shipment_id)
                .fetch_one(&mut *tx)
                .await?;
        let taken: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM dispatch.home_reservations
              WHERE driver_id = $1 AND move_date = $2 AND released_at IS NULL",
        )
        .bind(driver_id)
        .bind(move_date)
        .fetch_one(&mut *tx)
        .await?;
        if taken >= i64::from(max_jobs.map_or(1, |(m,)| m)) {
            tx.rollback().await?;
            return Ok(ReserveOutcome::DayFull);
        }

        sqlx::query(
            "INSERT INTO dispatch.home_reservations (shipment_id, tenant_id, driver_id, move_date)
             VALUES ($1, $2, $3, $4)",
        )
        .bind(shipment_id)
        .bind(tenant_id)
        .bind(driver_id)
        .bind(move_date)
        .execute(&mut *tx)
        .await?;
        sqlx::query("UPDATE dispatch.dispatch_queue SET status = 'pending' WHERE shipment_id = $1")
            .bind(shipment_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;

        let all_candidate_ids: Vec<Uuid> =
            sqlx::query_scalar("SELECT driver_id FROM dispatch.task_offer_candidates WHERE offer_id = $1")
                .bind(offer_id)
                .fetch_all(&self.pool)
                .await?;
        Ok(ReserveOutcome::Reserved { tenant_id, shipment_id, move_date, all_candidate_ids })
    }

    /// Reservations whose move starts before `by`, not yet activated:
    /// (tenant, shipment, driver).
    pub async fn due(&self, by: DateTime<Utc>) -> anyhow::Result<Vec<(Uuid, Uuid, Uuid)>> {
        let rows: Vec<(Uuid, Uuid, Uuid)> = sqlx::query_as(
            "SELECT r.tenant_id, r.shipment_id, r.driver_id
               FROM dispatch.home_reservations r
               JOIN dispatch.home_requirements h ON h.shipment_id = r.shipment_id
              WHERE r.activated_at IS NULL AND r.released_at IS NULL AND h.move_at <= $1",
        )
        .bind(by)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// A lead's reserved home moves, soonest first: what they have to survey
    /// and move, and where.
    pub async fn mine(&self, tenant_id: Uuid, driver_id: Uuid) -> anyhow::Result<Vec<LeadMove>> {
        let rows = sqlx::query(
            "SELECT r.shipment_id, r.move_date, r.activated_at, h.move_at, h.survey_at, h.trucks, h.helpers,
                    h.crew_total, h.large_estate, h.international,
                    q.customer_name, q.origin_address_line1, q.origin_city, q.dest_address_line1, q.dest_city,
                    q.tracking_number
               FROM dispatch.home_reservations r
               JOIN dispatch.home_requirements h ON h.shipment_id = r.shipment_id
               LEFT JOIN dispatch.dispatch_queue q ON q.shipment_id = r.shipment_id
              WHERE r.tenant_id = $1 AND r.driver_id = $2 AND r.released_at IS NULL
                AND h.move_at >= NOW() - INTERVAL '1 day'
              ORDER BY h.move_at",
        )
        .bind(tenant_id)
        .bind(driver_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| LeadMove {
                shipment_id: r.get("shipment_id"),
                tracking_number: r.get("tracking_number"),
                customer_name: r.get::<Option<String>, _>("customer_name").unwrap_or_default(),
                pickup: format!(
                    "{}, {}",
                    r.get::<Option<String>, _>("origin_address_line1").unwrap_or_default(),
                    r.get::<Option<String>, _>("origin_city").unwrap_or_default()
                ),
                dropoff: format!(
                    "{}, {}",
                    r.get::<Option<String>, _>("dest_address_line1").unwrap_or_default(),
                    r.get::<Option<String>, _>("dest_city").unwrap_or_default()
                ),
                move_at: r.get("move_at"),
                move_date: r.get("move_date"),
                survey_at: r.get("survey_at"),
                trucks: r.get("trucks"),
                helpers: r.get("helpers"),
                crew_total: r.get("crew_total"),
                large_estate: r.get("large_estate"),
                international: r.get("international"),
                activated: r.get::<Option<DateTime<Utc>>, _>("activated_at").is_some(),
            })
            .collect())
    }

    pub async fn mark_activated(&self, shipment_id: Uuid) -> anyhow::Result<()> {
        sqlx::query("UPDATE dispatch.home_reservations SET activated_at = NOW() WHERE shipment_id = $1")
            .bind(shipment_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn reservation(&self, shipment_id: Uuid) -> anyhow::Result<Option<ReservationView>> {
        let row = sqlx::query(
            "SELECT r.shipment_id, r.driver_id, r.move_date, r.reserved_at, r.activated_at,
                    COALESCE(d.first_name || ' ' || d.last_name, '') AS lead_name
               FROM dispatch.home_reservations r
               LEFT JOIN driver_ops.drivers d ON d.id = r.driver_id
              WHERE r.shipment_id = $1 AND r.released_at IS NULL",
        )
        .bind(shipment_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| ReservationView {
            shipment_id: r.get("shipment_id"),
            driver_id: r.get("driver_id"),
            lead_name: r.get::<String, _>("lead_name").trim().to_owned(),
            move_date: r.get("move_date"),
            reserved_at: r.get("reserved_at"),
            activated_at: r.get("activated_at"),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn req(trucks: i64, large: bool) -> HomeMoveRequirement {
        HomeMoveRequirement {
            trucks,
            helpers: 4,
            crew_total: trucks + 4,
            large_estate: large,
            international: false,
            move_at: Utc.with_ymd_and_hms(2026, 9, 21, 0, 0, 0).unwrap(),
            move_date: None,
            survey_at: None,
        }
    }

    #[test]
    fn a_second_truck_or_a_large_estate_needs_a_multi_truck_lead() {
        assert!(!needs_multi_truck(&req(1, false)));
        assert!(needs_multi_truck(&req(2, false)));
        assert!(needs_multi_truck(&req(1, true)));
    }

    #[test]
    fn the_day_comes_from_the_event_or_the_move_itself() {
        let mut r = req(1, false);
        assert_eq!(move_date_of(&r), NaiveDate::from_ymd_opt(2026, 9, 21).unwrap());
        r.move_date = NaiveDate::from_ymd_opt(2026, 9, 22);
        assert_eq!(move_date_of(&r), NaiveDate::from_ymd_opt(2026, 9, 22).unwrap());
        assert_eq!(weekday_of(NaiveDate::from_ymd_opt(2026, 9, 21).unwrap()), 0, "a Monday");
    }
}
