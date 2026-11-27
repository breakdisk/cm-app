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
    /// `role`: sole | captain | support | emergency.
    Reserved { tenant_id: Uuid, shipment_id: Uuid, move_date: NaiveDate, all_candidate_ids: Vec<Uuid>, role: String },
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
    /// What this lead is paid for their part of the move, after commission.
    pub lead_payout_cents: Option<i64>,
    /// sole | captain | support | emergency.
    pub role: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReservationView {
    pub shipment_id: Uuid,
    /// The primary lead: sole, or a joint move's captain.
    pub driver_id: Uuid,
    pub lead_name: String,
    pub move_date: NaiveDate,
    pub reserved_at: DateTime<Utc>,
    pub activated_at: Option<DateTime<Utc>>,
    pub role: String,
    /// Every lead on the move, the primary first.
    pub slots: Vec<SlotLead>,
}

/// One lead's part of a move.
#[derive(Debug, Clone, Serialize)]
pub struct SlotLead {
    pub driver_id: Uuid,
    pub lead_name: String,
    pub role: String,
    pub payout_cents: Option<i64>,
}

pub const ROLE_SOLE: &str = "sole";
pub const ROLE_CAPTAIN: &str = "captain";
pub const ROLE_SUPPORT: &str = "support";
pub const ROLE_EMERGENCY: &str = "emergency";
/// The slot an offer fills: the move itself (taken sole or as captain), a
/// joint move's second truck, or an addendum's extra truck.
pub const SLOT_PRIMARY: &str = "primary";

/// Model B: a two-truck move may be taken by two single-truck leads.
pub fn joint_allowed(req: &HomeMoveRequirement) -> bool {
    req.trucks == 2
}

/// The crew each lead of a joint move brings: the captain the larger half.
pub fn captain_helpers(req: &HomeMoveRequirement) -> i64 {
    (req.helpers + 1) / 2
}

pub fn support_helpers(req: &HomeMoveRequirement) -> i64 {
    req.helpers / 2
}

/// An extra truck comes with the per-truck crew: a driver and two helpers.
pub const EMERGENCY_HELPERS: i64 = 2;

/// Who a primary claim makes the lead: sole when they can take the move
/// whole (a single-truck move, or a multi-truck lead with the trucks),
/// otherwise the captain of a joint move. Side slots keep their own name.
pub fn role_for(slot: &str, req_trucks: i64, large_estate: bool, multi_truck_capable: bool, fleet_trucks: i64) -> String {
    if slot != SLOT_PRIMARY {
        return slot.to_owned();
    }
    let whole = if req_trucks <= 1 && !large_estate { true } else { multi_truck_capable && fleet_trucks >= req_trucks.max(1) };
    if whole { ROLE_SOLE } else { ROLE_CAPTAIN }.to_owned()
}

/// What a lead must have to be offered a slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotNeed {
    pub multi_truck: bool,
    pub min_trucks: i64,
    pub min_helpers: i64,
    /// Leads already on the move.
    pub exclude: Vec<Uuid>,
    /// Not on a job right now: an emergency truck has to come at once.
    pub idle_only: bool,
}

impl SlotNeed {
    /// The whole move, by one lead.
    pub fn whole(req: &HomeMoveRequirement) -> Self {
        let multi = needs_multi_truck(req);
        Self { multi_truck: multi, min_trucks: if multi { req.trucks.max(1) } else { 1 }, min_helpers: req.helpers, exclude: Vec::new(), idle_only: false }
    }

    pub fn captain(req: &HomeMoveRequirement) -> Self {
        Self { multi_truck: false, min_trucks: 1, min_helpers: captain_helpers(req), exclude: Vec::new(), idle_only: false }
    }

    pub fn support(req: &HomeMoveRequirement, exclude: Vec<Uuid>) -> Self {
        Self { multi_truck: false, min_trucks: 1, min_helpers: support_helpers(req), exclude, idle_only: false }
    }

    pub fn emergency(exclude: Vec<Uuid>) -> Self {
        Self { multi_truck: false, min_trucks: 1, min_helpers: EMERGENCY_HELPERS, exclude, idle_only: true }
    }
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
                     move_at, move_date, survey_at, lead_payout_cents, captain_payout_cents, support_payout_cents)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
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
        .bind(req.lead_payout_cents)
        .bind(req.captain_payout_cents)
        .bind(req.support_payout_cents)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn requirement(&self, shipment_id: Uuid) -> anyhow::Result<Option<HomeMoveRequirement>> {
        let row = sqlx::query(
            "SELECT trucks, helpers, crew_total, large_estate, international, move_at, move_date, survey_at,
                    lead_payout_cents, captain_payout_cents, support_payout_cents
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
            lead_payout_cents: r.get("lead_payout_cents"),
            captain_payout_cents: r.get("captain_payout_cents"),
            support_payout_cents: r.get("support_payout_cents"),
        }))
    }

    /// A lead has taken the move itself (sole, or a joint move's captain).
    pub async fn is_reserved(&self, shipment_id: Uuid) -> anyhow::Result<bool> {
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM dispatch.home_reservations
              WHERE shipment_id = $1 AND released_at IS NULL AND role IN ('sole', 'captain')",
        )
        .bind(shipment_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(n > 0)
    }

    /// Every home-move lead in the tenant who can meet this move: coverage,
    /// trucks, helpers, working that day, not off, and not already full.
    pub async fn eligible_leads(&self, tenant_id: Uuid, req: &HomeMoveRequirement) -> anyhow::Result<Vec<HomeLead>> {
        self.eligible_for(tenant_id, req.international, move_date_of(req), &SlotNeed::whole(req)).await
    }

    /// Leads who can fill a slot on `date`: onboarded for home moves, the
    /// right coverage, the trucks and helpers the slot needs, working that day,
    /// not off, not already full, not already on the move — and for an
    /// emergency truck, not on a job right now.
    pub async fn eligible_for(&self, tenant_id: Uuid, international: bool, date: NaiveDate, need: &SlotNeed) -> anyhow::Result<Vec<HomeLead>> {
        let rows = sqlx::query(
            "SELECT p.driver_id, d.lat, d.lng
               FROM driver_ops.provider_profiles p
               JOIN driver_ops.drivers d ON d.id = p.driver_id AND d.is_active
              WHERE p.tenant_id = $1
                AND 'home_move' = ANY(p.service_lines)
                AND (CASE WHEN $2 THEN 'international' ELSE 'local' END) = ANY(p.coverage)
                AND (NOT $3 OR p.multi_truck_capable)
                AND p.fleet_trucks >= $4
                AND p.registered_helpers >= $5
                AND $6::smallint = ANY(p.working_days)
                AND NOT EXISTS (SELECT 1 FROM driver_ops.provider_off_days o
                                 WHERE o.driver_id = p.driver_id AND o.day = $7)
                AND (SELECT COUNT(*) FROM dispatch.home_reservations r
                      WHERE r.driver_id = p.driver_id AND r.move_date = $7 AND r.released_at IS NULL)
                    < p.max_daily_jobs
                AND NOT (p.driver_id = ANY($8))
                AND (NOT $9 OR NOT EXISTS (SELECT 1 FROM dispatch.driver_assignments a
                                            WHERE a.driver_id = p.driver_id AND a.status IN ('pending', 'accepted')))",
        )
        .bind(tenant_id)
        .bind(international)
        .bind(need.multi_truck)
        .bind(i32::try_from(need.min_trucks).unwrap_or(i32::MAX))
        .bind(i32::try_from(need.min_helpers).unwrap_or(i32::MAX))
        .bind(weekday_of(date))
        .bind(date)
        .bind(&need.exclude)
        .bind(need.idle_only)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| HomeLead { driver_id: r.get("driver_id"), lat: r.get("lat"), lng: r.get("lng") })
            .collect())
    }

    /// The slot an offer fills; None when it is not a home move's offer.
    pub async fn offer_slot(&self, offer_id: Uuid) -> anyhow::Result<Option<String>> {
        let row: Option<(Option<String>,)> = sqlx::query_as(
            "SELECT s.role FROM dispatch.task_offers o
               JOIN dispatch.home_requirements h ON h.shipment_id = o.shipment_id
               LEFT JOIN dispatch.home_offer_slots s ON s.offer_id = o.id
              WHERE o.id = $1",
        )
        .bind(offer_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(role,)| role.unwrap_or_else(|| SLOT_PRIMARY.to_owned())))
    }

    pub async fn mark_offer_slot(&self, offer_id: Uuid, shipment_id: Uuid, slot: &str) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO dispatch.home_offer_slots (offer_id, shipment_id, role) VALUES ($1, $2, $3)
             ON CONFLICT (offer_id) DO NOTHING",
        )
        .bind(offer_id)
        .bind(shipment_id)
        .bind(slot)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Leads already holding a slot on the move.
    pub async fn reserved_drivers(&self, shipment_id: Uuid) -> anyhow::Result<Vec<Uuid>> {
        Ok(sqlx::query_scalar(
            "SELECT driver_id FROM dispatch.home_reservations WHERE shipment_id = $1 AND released_at IS NULL",
        )
        .bind(shipment_id)
        .fetch_all(&self.pool)
        .await?)
    }

    /// Whether this offer is for a home move.
    pub async fn offer_is_home(&self, offer_id: Uuid) -> anyhow::Result<bool> {
        Ok(self.offer_slot(offer_id).await?.is_some())
    }

    /// The lead's claim on a home offer: the offer closes, the lead is
    /// reserved for the day, the queue row goes back to pending for the
    /// activation sweep. The lead's profile row is locked so two claims for
    /// the same day cannot both pass the jobs-a-day check.
    pub async fn reserve(&self, offer_id: Uuid, driver_id: Uuid) -> anyhow::Result<ReserveOutcome> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET LOCAL lock_timeout = '250ms'").execute(&mut *tx).await?;

        let candidate: Option<(Option<i64>,)> =
            sqlx::query_as("SELECT payout_cents FROM dispatch.task_offer_candidates WHERE offer_id = $1 AND driver_id = $2")
                .bind(offer_id)
                .bind(driver_id)
                .fetch_optional(&mut *tx)
                .await?;
        let Some((payout_cents,)) = candidate else {
            return Ok(ReserveOutcome::NotACandidate);
        };
        let slot: String = sqlx::query_scalar::<_, String>("SELECT role FROM dispatch.home_offer_slots WHERE offer_id = $1")
            .bind(offer_id)
            .fetch_optional(&mut *tx)
            .await?
            .unwrap_or_else(|| SLOT_PRIMARY.to_owned());

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

        let profile: Option<(i32, bool, i32)> = sqlx::query_as(
            "SELECT max_daily_jobs, multi_truck_capable, fleet_trucks FROM driver_ops.provider_profiles
              WHERE driver_id = $1 FOR UPDATE",
        )
        .bind(driver_id)
        .fetch_optional(&mut *tx)
        .await?;
        let (move_date, trucks, large_estate): (NaiveDate, i32, bool) = sqlx::query_as(
            "SELECT move_date, trucks, large_estate FROM dispatch.home_requirements WHERE shipment_id = $1",
        )
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
        let (max_jobs, multi, fleet) = profile.unwrap_or((1, false, 1));
        if taken >= i64::from(max_jobs) {
            tx.rollback().await?;
            return Ok(ReserveOutcome::DayFull);
        }
        let role = role_for(&slot, i64::from(trucks), large_estate, multi, i64::from(fleet));

        let inserted = sqlx::query(
            "INSERT INTO dispatch.home_reservations (shipment_id, tenant_id, driver_id, move_date, role, payout_cents)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(shipment_id)
        .bind(tenant_id)
        .bind(driver_id)
        .bind(move_date)
        .bind(&role)
        .bind(payout_cents)
        .execute(&mut *tx)
        .await;
        if let Err(e) = inserted {
            // One primary per move, one slot per lead on it: someone got here first.
            if e.as_database_error().and_then(|d| d.code()).is_some_and(|c| c == "23505") {
                tx.rollback().await?;
                return Ok(ReserveOutcome::AlreadyTaken);
            }
            return Err(e.into());
        }
        // The move itself is now held for its lead: back to pending, which
        // the activation sweep turns into the assignment. A side slot never
        // touches the queue — the primary owns it.
        if slot == SLOT_PRIMARY {
            sqlx::query("UPDATE dispatch.dispatch_queue SET status = 'pending' WHERE shipment_id = $1")
                .bind(shipment_id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;

        let all_candidate_ids: Vec<Uuid> =
            sqlx::query_scalar("SELECT driver_id FROM dispatch.task_offer_candidates WHERE offer_id = $1")
                .bind(offer_id)
                .fetch_all(&self.pool)
                .await?;
        Ok(ReserveOutcome::Reserved { tenant_id, shipment_id, move_date, all_candidate_ids, role })
    }

    /// Reservations whose move starts before `by`, not yet activated:
    /// (tenant, shipment, driver, the lead's net pay).
    /// Primary reservations (sole or captain) whose move starts before `by`,
    /// not yet activated: (tenant, shipment, driver, the pay the lead accepted).
    /// Side slots are paid as credits on delivery, not by an assignment.
    pub async fn due(&self, by: DateTime<Utc>) -> anyhow::Result<Vec<(Uuid, Uuid, Uuid, Option<i64>)>> {
        let rows: Vec<(Uuid, Uuid, Uuid, Option<i64>)> = sqlx::query_as(
            "SELECT r.tenant_id, r.shipment_id, r.driver_id, COALESCE(r.payout_cents, h.lead_payout_cents)
               FROM dispatch.home_reservations r
               JOIN dispatch.home_requirements h ON h.shipment_id = r.shipment_id
              WHERE r.activated_at IS NULL AND r.released_at IS NULL
                AND r.role IN ('sole', 'captain') AND h.move_at <= $1",
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
                    h.crew_total, h.large_estate, h.international, r.role,
                    COALESCE(r.payout_cents, h.lead_payout_cents) AS lead_payout_cents,
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
                lead_payout_cents: r.get("lead_payout_cents"),
                role: r.get("role"),
            })
            .collect())
    }

    pub async fn mark_activated(&self, shipment_id: Uuid) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE dispatch.home_reservations SET activated_at = NOW()
              WHERE shipment_id = $1 AND role IN ('sole', 'captain') AND released_at IS NULL",
        )
        .bind(shipment_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn reservation(&self, shipment_id: Uuid) -> anyhow::Result<Option<ReservationView>> {
        let rows = sqlx::query(
            "SELECT r.shipment_id, r.driver_id, r.move_date, r.reserved_at, r.activated_at, r.role, r.payout_cents,
                    COALESCE(d.first_name || ' ' || d.last_name, '') AS lead_name
               FROM dispatch.home_reservations r
               LEFT JOIN driver_ops.drivers d ON d.id = r.driver_id
              WHERE r.shipment_id = $1 AND r.released_at IS NULL
              ORDER BY (r.role IN ('sole', 'captain')) DESC, r.reserved_at",
        )
        .bind(shipment_id)
        .fetch_all(&self.pool)
        .await?;
        let slots: Vec<SlotLead> = rows
            .iter()
            .map(|r| SlotLead {
                driver_id: r.get("driver_id"),
                lead_name: r.get::<String, _>("lead_name").trim().to_owned(),
                role: r.get("role"),
                payout_cents: r.get("payout_cents"),
            })
            .collect();
        let Some(primary) = rows.iter().find(|r| {
            let role: String = r.get("role");
            role == ROLE_SOLE || role == ROLE_CAPTAIN
        }) else {
            return Ok(None);
        };
        Ok(Some(ReservationView {
            shipment_id: primary.get("shipment_id"),
            driver_id: primary.get("driver_id"),
            lead_name: primary.get::<String, _>("lead_name").trim().to_owned(),
            move_date: primary.get("move_date"),
            reserved_at: primary.get("reserved_at"),
            activated_at: primary.get("activated_at"),
            role: primary.get("role"),
            slots,
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
            lead_payout_cents: None,
            captain_payout_cents: None,
            support_payout_cents: None,
        }
    }

    #[test]
    fn a_primary_claim_is_sole_when_the_lead_can_take_it_whole() {
        // A one-truck move: anyone who is offered it takes it whole.
        assert_eq!(role_for(SLOT_PRIMARY, 1, false, false, 1), ROLE_SOLE);
        // Two trucks: an enterprise lead with both takes it whole ...
        assert_eq!(role_for(SLOT_PRIMARY, 2, false, true, 2), ROLE_SOLE);
        // ... a single-truck lead is the captain of a joint mission.
        assert_eq!(role_for(SLOT_PRIMARY, 2, false, false, 1), ROLE_CAPTAIN);
        assert_eq!(role_for(SLOT_PRIMARY, 2, true, true, 1), ROLE_CAPTAIN);
        // Side slots keep their name.
        assert_eq!(role_for(ROLE_SUPPORT, 2, false, true, 3), ROLE_SUPPORT);
        assert_eq!(role_for(ROLE_EMERGENCY, 1, false, false, 1), ROLE_EMERGENCY);
    }

    #[test]
    fn a_joint_mission_splits_the_crew_the_captain_the_larger_half() {
        let mut r = req(2, true);
        r.helpers = 5;
        assert!(joint_allowed(&r));
        assert_eq!((captain_helpers(&r), support_helpers(&r)), (3, 2));
        assert!(!joint_allowed(&req(1, false)));
        assert!(!joint_allowed(&req(3, true)), "three trucks is an enterprise job");
        // A captain needs only one truck; the whole move needs both.
        assert_eq!(SlotNeed::captain(&r).min_trucks, 1);
        assert_eq!(SlotNeed::whole(&r).min_trucks, 2);
        assert!(SlotNeed::whole(&r).multi_truck);
        assert!(SlotNeed::emergency(vec![]).idle_only);
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
