//! Gig task-offer persistence — broadcast fan-out + the atomic claim CAS.
//!
//! The claim is a single-row compare-and-swap on `task_offers.status`:
//! Postgres row locking serializes concurrent grabbers; losers block only for
//! the winner's (lean) transaction duration, then re-evaluate the WHERE and
//! get zero rows. The one-active-assignment partial unique index
//! (`idx_one_active_assignment_per_driver`, migration 0012) closes the
//! cross-offer double-grab race at the constraint level — a driver tapping
//! two offers simultaneously gets exactly one.
//!
//! NO network I/O happens inside the claim transaction — Kafka/FCM publishing
//! is the caller's post-commit job. `SET LOCAL lock_timeout` sheds blocked
//! claimers fast if a winner's commit ever stalls.

use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use uuid::Uuid;

/// A task offer row as stored (denormalized card fields included).
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct TaskOfferRow {
    pub id:                Uuid,
    pub tenant_id:         Uuid,
    pub shipment_id:       Uuid,
    pub queue_id:          Uuid,
    pub status:            String,
    pub wave:              i32,
    pub expires_at:        DateTime<Utc>,
    pub claimed_by:        Option<Uuid>,
    pub merchant_name:     String,
    pub delivery_category: String,
    pub weight_grams:      i64,
    pub tracking_number:   String,
    pub customer_name:     String,
    pub pickup_address:    String,
    pub delivery_address:  String,
    pub cod_amount_cents:  Option<i64>,
    pub pickup_lat:        Option<f64>,
    pub pickup_lng:        Option<f64>,
    pub delivery_lat:      Option<f64>,
    pub delivery_lng:      Option<f64>,
}

/// Candidate to fan an offer out to (payout snapshotted per driver).
#[derive(Debug, Clone)]
pub struct OfferCandidateRow {
    pub driver_id:    Uuid,
    pub payout_cents: Option<i64>,
}

/// What the driver's app needs to render an open grab card.
#[derive(Debug, serde::Serialize)]
pub struct OpenOfferView {
    #[serde(flatten)]
    pub offer:        TaskOfferRow,
    /// The authenticated driver's own contractual payout for this offer.
    pub payout_cents: Option<i64>,
}

/// Deterministic claim outcomes — mapped to typed 409s at the HTTP layer.
#[derive(Debug)]
pub enum ClaimOutcome {
    Won {
        tenant_id:         Uuid,
        shipment_id:       Uuid,
        payout_cents:      Option<i64>,
        assignment_id:     Uuid,
        route_id:          Uuid,
        all_candidate_ids: Vec<Uuid>,
    },
    /// CAS lost: another driver claimed it first.
    AlreadyTaken,
    /// Offer TTL elapsed before the tap landed.
    Expired,
    /// 23505 on idx_one_active_assignment_per_driver — the driver already
    /// holds an active assignment (e.g. won a different offer concurrently).
    DriverBusy,
    /// Caller was never offered this task (or offer id is unknown).
    NotACandidate,
}

pub struct PgTaskOfferRepository {
    pool: PgPool,
}

impl PgTaskOfferRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }

    /// Insert the offer + its candidate rows atomically and return the offer id.
    /// Re-broadcast (wave escalation) goes through `escalate_wave` instead.
    pub async fn create_offer(
        &self,
        offer: &TaskOfferRow,
        candidates: &[OfferCandidateRow],
    ) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            r#"INSERT INTO dispatch.task_offers
                   (id, tenant_id, shipment_id, queue_id, status, wave, expires_at,
                    merchant_name, delivery_category, weight_grams, tracking_number,
                    customer_name, pickup_address, delivery_address, cod_amount_cents,
                    pickup_lat, pickup_lng, delivery_lat, delivery_lng)
               VALUES ($1,$2,$3,$4,'open',$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18)"#,
        )
        .bind(offer.id)
        .bind(offer.tenant_id)
        .bind(offer.shipment_id)
        .bind(offer.queue_id)
        .bind(offer.wave)
        .bind(offer.expires_at)
        .bind(&offer.merchant_name)
        .bind(&offer.delivery_category)
        .bind(offer.weight_grams)
        .bind(&offer.tracking_number)
        .bind(&offer.customer_name)
        .bind(&offer.pickup_address)
        .bind(&offer.delivery_address)
        .bind(offer.cod_amount_cents)
        .bind(offer.pickup_lat)
        .bind(offer.pickup_lng)
        .bind(offer.delivery_lat)
        .bind(offer.delivery_lng)
        .execute(&mut *tx)
        .await?;

        for c in candidates {
            sqlx::query(
                r#"INSERT INTO dispatch.task_offer_candidates
                       (offer_id, driver_id, wave, payout_cents)
                   VALUES ($1,$2,$3,$4)
                   ON CONFLICT (offer_id, driver_id) DO NOTHING"#,
            )
            .bind(offer.id)
            .bind(c.driver_id)
            .bind(offer.wave)
            .bind(c.payout_cents)
            .execute(&mut *tx)
            .await?;
        }

        // Atomically take the queue row out of 'pending' while the offer is
        // live. Without this, the 90 s dispatch sweep could 1:1-assign a
        // shipment that gig drivers are mid-grab on (double assignment). On
        // unclaimed expiry the sweeper resets it to 'pending' for 1:1 fallback;
        // on a winning claim it stays 'dispatched'.
        sqlx::query(
            "UPDATE dispatch.dispatch_queue
             SET status = 'dispatched', dispatched_at = NOW()
             WHERE shipment_id = $1 AND status = 'pending'",
        )
        .bind(offer.shipment_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }

    /// The grab. One transaction: candidate check → CAS → route + assignment
    /// inserts → queue flip. Deterministic outcomes; no network I/O inside.
    pub async fn claim(&self, offer_id: Uuid, driver_id: Uuid) -> anyhow::Result<ClaimOutcome> {
        let mut tx = self.pool.begin().await?;

        // Shed blocked claimers fast: losers waiting on the winner's row lock
        // time out instead of stacking up on the connection pool.
        sqlx::query("SET LOCAL lock_timeout = '250ms'")
            .execute(&mut *tx)
            .await?;

        // Caller must have been offered this task — also fetches THEIR payout.
        let candidate: Option<(Option<i64>,)> = sqlx::query_as(
            "SELECT payout_cents FROM dispatch.task_offer_candidates
             WHERE offer_id = $1 AND driver_id = $2",
        )
        .bind(offer_id)
        .bind(driver_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some((payout_cents,)) = candidate else {
            return Ok(ClaimOutcome::NotACandidate);
        };

        // The CAS. Zero rows = lost the race (taken or expired).
        let won = sqlx::query(
            r#"UPDATE dispatch.task_offers
               SET status = 'claimed', claimed_by = $2, claimed_at = NOW()
               WHERE id = $1 AND status = 'open' AND expires_at > NOW()
               RETURNING tenant_id, shipment_id"#,
        )
        .bind(offer_id)
        .bind(driver_id)
        .fetch_optional(&mut *tx)
        .await?;

        let Some(row) = won else {
            // Distinguish "taken" from "expired" for honest UX copy.
            let status: Option<(String, DateTime<Utc>)> = sqlx::query_as(
                "SELECT status, expires_at FROM dispatch.task_offers WHERE id = $1",
            )
            .bind(offer_id)
            .fetch_optional(&mut *tx)
            .await?;
            return Ok(match status {
                Some((s, _)) if s == "claimed" => ClaimOutcome::AlreadyTaken,
                Some((s, exp)) if s == "open" && exp <= Utc::now() => ClaimOutcome::Expired,
                Some(_) => ClaimOutcome::Expired,
                None => ClaimOutcome::NotACandidate,
            });
        };
        let tenant_id:   Uuid = row.get("tenant_id");
        let shipment_id: Uuid = row.get("shipment_id");

        // Route born in_progress: the grab IS the acceptance (no separate
        // accept step like the 1:1 flow). Mirrors accept_assignment's effect.
        let route_id      = Uuid::new_v4();
        let assignment_id = Uuid::new_v4();
        sqlx::query(
            r#"INSERT INTO dispatch.routes
                   (id, tenant_id, driver_id, vehicle_id, status,
                    total_distance_km, estimated_duration_minutes, created_at, started_at)
               VALUES ($1,$2,$3,$4,'in_progress',0,0,NOW(),NOW())"#,
        )
        .bind(route_id)
        .bind(tenant_id)
        .bind(driver_id)
        .bind(Uuid::nil())
        .execute(&mut *tx)
        .await?;

        // idx_one_active_assignment_per_driver fires here if the driver
        // grabbed another offer concurrently → DriverBusy (tx rolls back,
        // this offer stays open for everyone else).
        let assignment_insert = sqlx::query(
            r#"INSERT INTO dispatch.driver_assignments
                   (id, tenant_id, driver_id, route_id, status, assigned_at, accepted_at)
               VALUES ($1,$2,$3,$4,'accepted',NOW(),NOW())"#,
        )
        .bind(assignment_id)
        .bind(tenant_id)
        .bind(driver_id)
        .bind(route_id)
        .execute(&mut *tx)
        .await;

        if let Err(e) = assignment_insert {
            let is_unique_violation = e
                .as_database_error()
                .and_then(|d| d.code())
                .is_some_and(|c| c == "23505");
            if is_unique_violation {
                tx.rollback().await?;
                return Ok(ClaimOutcome::DriverBusy);
            }
            return Err(e.into());
        }

        sqlx::query(
            "UPDATE dispatch.dispatch_queue
             SET status = 'dispatched', dispatched_at = NOW()
             WHERE shipment_id = $1",
        )
        .bind(shipment_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        // Post-commit read — losers' FCM fan-in targets (all waves).
        let all_candidate_ids: Vec<Uuid> = sqlx::query_scalar(
            "SELECT driver_id FROM dispatch.task_offer_candidates WHERE offer_id = $1",
        )
        .bind(offer_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(ClaimOutcome::Won {
            tenant_id,
            shipment_id,
            payout_cents,
            assignment_id,
            route_id,
            all_candidate_ids,
        })
    }

    /// Client-verified impression — first render only (COALESCE keeps the
    /// earliest timestamp under FCM redelivery / recomposition).
    pub async fn mark_seen(&self, offer_id: Uuid, driver_id: Uuid) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE dispatch.task_offer_candidates
             SET seen_at = COALESCE(seen_at, NOW())
             WHERE offer_id = $1 AND driver_id = $2",
        )
        .bind(offer_id)
        .bind(driver_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Explicit pass — never re-offered in later waves; penalty-free (does
    /// NOT touch decline_count: that counter is for targeted 1:1 declines).
    pub async fn record_pass(&self, offer_id: Uuid, driver_id: Uuid) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE dispatch.task_offer_candidates
             SET response = 'passed'
             WHERE offer_id = $1 AND driver_id = $2",
        )
        .bind(offer_id)
        .bind(driver_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Live offers for the authenticated driver (app-restart recovery path —
    /// FCM must not be a single point of failure).
    pub async fn find_open_for_driver(&self, driver_id: Uuid) -> anyhow::Result<Vec<OpenOfferView>> {
        let rows = sqlx::query(
            r#"SELECT o.id, o.tenant_id, o.shipment_id, o.queue_id, o.status, o.wave,
                      o.expires_at, o.claimed_by, o.merchant_name, o.delivery_category,
                      o.weight_grams, o.tracking_number, o.customer_name,
                      o.pickup_address, o.delivery_address, o.cod_amount_cents,
                      o.pickup_lat, o.pickup_lng, o.delivery_lat, o.delivery_lng,
                      c.payout_cents
               FROM dispatch.task_offers o
               JOIN dispatch.task_offer_candidates c ON c.offer_id = o.id
               WHERE c.driver_id = $1
                 AND c.response = 'none'
                 AND o.status = 'open'
                 AND o.expires_at > NOW()
               ORDER BY o.expires_at ASC"#,
        )
        .bind(driver_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| OpenOfferView {
            offer: TaskOfferRow {
                id:                r.get("id"),
                tenant_id:         r.get("tenant_id"),
                shipment_id:       r.get("shipment_id"),
                queue_id:          r.get("queue_id"),
                status:            r.get("status"),
                wave:              r.get("wave"),
                expires_at:        r.get("expires_at"),
                claimed_by:        r.get("claimed_by"),
                merchant_name:     r.get("merchant_name"),
                delivery_category: r.get("delivery_category"),
                weight_grams:      r.get("weight_grams"),
                tracking_number:   r.get("tracking_number"),
                customer_name:     r.get("customer_name"),
                pickup_address:    r.get("pickup_address"),
                delivery_address:  r.get("delivery_address"),
                cod_amount_cents:  r.get("cod_amount_cents"),
                pickup_lat:        r.get("pickup_lat"),
                pickup_lng:        r.get("pickup_lng"),
                delivery_lat:      r.get("delivery_lat"),
                delivery_lng:      r.get("delivery_lng"),
            },
            payout_cents: r.get("payout_cents"),
        }).collect())
    }

    /// Expired-but-open offers due for wave escalation or terminal expiry.
    pub async fn list_expired_open(&self) -> anyhow::Result<Vec<TaskOfferRow>> {
        let rows = sqlx::query_as::<_, TaskOfferRow>(
            r#"SELECT id, tenant_id, shipment_id, queue_id, status, wave, expires_at,
                      claimed_by, merchant_name, delivery_category, weight_grams,
                      tracking_number, customer_name, pickup_address, delivery_address,
                      cod_amount_cents, pickup_lat, pickup_lng, delivery_lat, delivery_lng
               FROM dispatch.task_offers
               WHERE status = 'open' AND expires_at <= NOW()
               ORDER BY expires_at ASC"#,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Drivers already offered this task in ANY wave — excluded from
    /// escalation re-selection (passed drivers stay excluded; non-responders
    /// keep their existing candidate row and simply see the extended TTL).
    pub async fn candidate_ids(&self, offer_id: Uuid) -> anyhow::Result<Vec<Uuid>> {
        Ok(sqlx::query_scalar(
            "SELECT driver_id FROM dispatch.task_offer_candidates WHERE offer_id = $1",
        )
        .bind(offer_id)
        .fetch_all(&self.pool)
        .await?)
    }

    /// Escalate to the next wave: extend the TTL and add new candidates.
    /// Only fires while the offer is still open (claim wins any race).
    pub async fn escalate_wave(
        &self,
        offer_id: Uuid,
        new_wave: i32,
        new_expires_at: DateTime<Utc>,
        new_candidates: &[OfferCandidateRow],
    ) -> anyhow::Result<bool> {
        let mut tx = self.pool.begin().await?;
        let updated = sqlx::query(
            "UPDATE dispatch.task_offers
             SET wave = $2, expires_at = $3
             WHERE id = $1 AND status = 'open'",
        )
        .bind(offer_id)
        .bind(new_wave)
        .bind(new_expires_at)
        .execute(&mut *tx)
        .await?;
        if updated.rows_affected() == 0 {
            tx.rollback().await?;
            return Ok(false);
        }
        for c in new_candidates {
            sqlx::query(
                r#"INSERT INTO dispatch.task_offer_candidates
                       (offer_id, driver_id, wave, payout_cents)
                   VALUES ($1,$2,$3,$4)
                   ON CONFLICT (offer_id, driver_id) DO NOTHING"#,
            )
            .bind(offer_id)
            .bind(c.driver_id)
            .bind(new_wave)
            .bind(c.payout_cents)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(true)
    }

    /// Terminal expiry. Returns all candidate ids for the offer_closed
    /// fan-out, or None if the offer was claimed in the meantime.
    pub async fn expire(&self, offer_id: Uuid) -> anyhow::Result<Option<Vec<Uuid>>> {
        let updated = sqlx::query(
            "UPDATE dispatch.task_offers
             SET status = 'expired'
             WHERE id = $1 AND status = 'open'",
        )
        .bind(offer_id)
        .execute(&self.pool)
        .await?;
        if updated.rows_affected() == 0 {
            return Ok(None);
        }
        Ok(Some(self.candidate_ids(offer_id).await?))
    }

    /// (verified impressions, claims) for the gig acceptance-rate strip.
    pub async fn offer_stats_for_driver(&self, driver_id: Uuid) -> anyhow::Result<(i64, i64)> {
        let row = sqlx::query(
            r#"SELECT
                   COUNT(*) FILTER (WHERE c.seen_at IS NOT NULL)::bigint AS seen,
                   COUNT(*) FILTER (WHERE o.claimed_by = $1)::bigint     AS claimed
               FROM dispatch.task_offer_candidates c
               JOIN dispatch.task_offers o ON o.id = c.offer_id
               WHERE c.driver_id = $1"#,
        )
        .bind(driver_id)
        .fetch_one(&self.pool)
        .await?;
        Ok((row.get("seen"), row.get("claimed")))
    }
}
