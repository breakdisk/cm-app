//! Gig offer broadcast ("grab") orchestration.
//!
//! Broadcast waves: an offer fans out to the top-N nearest eligible PART-TIME
//! drivers simultaneously (30 s TTL). First atomic claim wins; the unclaimed
//! offer escalates through widening waves (3 → 6 → 10 km), then expires onto
//! the ops console — the manual dispatch path remains the non-AI fallback.
//!
//! Decline semantics differ from the targeted 1:1 flow on purpose: an
//! explicit "pass" only excludes the driver from later waves of THIS offer.
//! It never touches `decline_count` — not tapping an offer that nine other
//! drivers also saw is not a refusal. The gig performance metric is the
//! acceptance rate over impression-verified offers instead.

use std::sync::Arc;
use chrono::{Duration, Utc};
use logisticos_errors::{AppError, AppResult};
use logisticos_events::{envelope::Event, producer::KafkaProducer, topics};
use logisticos_types::{Coordinates, DriverId, TenantId};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::domain::repositories::DriverAvailabilityRepository;
use crate::domain::value_objects::vehicle_can_carry;
use crate::infrastructure::db::{
    ClaimOutcome, ComplianceCache, DispatchQueueRepository, DispatchQueueRow,
    OfferCandidateRow, OpenOfferView, PgTaskOfferRepository, TaskOfferRow,
};

/// Search radius per wave (km). Wave numbers are 1-based.
pub const WAVE_RADIUS_KM: [f64; 3] = [3.0, 6.0, 10.0];
/// How long each wave stays grabbable.
pub const OFFER_TTL_SECS: i64 = 30;
/// Candidates fanned out per wave — bounds claim contention and FCM volume.
pub const OFFER_WAVE_SIZE: usize = 10;
pub const OFFER_MAX_WAVES: i32 = 3;
/// A home move is offered weeks ahead: leads get an hour, not thirty seconds.
pub const HOME_OFFER_TTL_SECS: i64 = 3_600;
/// The widest a home offer goes at once.
pub const HOME_OFFER_MAX_LEADS: usize = 25;

/// Error markers the HTTP layer maps to typed 409s.
pub const ERR_OFFER_TAKEN: &str = "OFFER_TAKEN";
pub const ERR_DRIVER_BUSY: &str = "DRIVER_BUSY";

pub struct OfferService {
    offer_repo:        Arc<PgTaskOfferRepository>,
    queue_repo:        Arc<dyn DispatchQueueRepository>,
    driver_avail_repo: Arc<dyn DriverAvailabilityRepository>,
    kafka:             Arc<KafkaProducer>,
    compliance_cache:  Option<Arc<Mutex<ComplianceCache>>>,
    home:              Option<Arc<crate::infrastructure::db::HomeRepo>>,
}

impl OfferService {
    pub fn new(
        offer_repo:        Arc<PgTaskOfferRepository>,
        queue_repo:        Arc<dyn DispatchQueueRepository>,
        driver_avail_repo: Arc<dyn DriverAvailabilityRepository>,
        kafka:             Arc<KafkaProducer>,
        compliance_cache:  Option<Arc<Mutex<ComplianceCache>>>,
    ) -> Self {
        Self { offer_repo, queue_repo, driver_avail_repo, kafka, compliance_cache, home: None }
    }

    #[must_use]
    pub fn with_home(mut self, home: Arc<crate::infrastructure::db::HomeRepo>) -> Self {
        self.home = Some(home);
        self
    }

    /// Offer a home move to every lead who can take it: onboarded for home
    /// moves, the right coverage, enough trucks and helpers, working that
    /// day and not full. Nearest first where a last position is known. One
    /// long wave, never escalated to the geo pool.
    async fn broadcast_home(
        &self,
        home: &crate::infrastructure::db::HomeRepo,
        tenant_id: TenantId,
        queue_item: &DispatchQueueRow,
    ) -> AppResult<TaskOfferRow> {
        let req = home.requirement(queue_item.shipment_id).await.map_err(AppError::Internal)?.ok_or_else(|| {
            AppError::BusinessRule("This home move has no crew requirement on record — it cannot be offered".into())
        })?;
        if home.is_reserved(queue_item.shipment_id).await.map_err(AppError::Internal)? {
            return Err(AppError::BusinessRule("A lead has already reserved this home move".into()));
        }
        let mut leads = home.eligible_leads(tenant_id.inner(), &req).await.map_err(AppError::Internal)?;
        // Model B: a two-truck move is also offered to single-truck leads, as
        // the captain of a joint mission; the enterprise leads above take it whole.
        let whole: std::collections::HashSet<Uuid> = leads.iter().map(|l| l.driver_id).collect();
        if crate::infrastructure::db::home_repo::joint_allowed(&req) {
            let date = crate::infrastructure::db::home_repo::move_date_of(&req);
            let captains = home
                .eligible_for(tenant_id.inner(), req.international, date, &crate::infrastructure::db::home_repo::SlotNeed::captain(&req))
                .await
                .map_err(AppError::Internal)?;
            leads.extend(captains.into_iter().filter(|c| !whole.contains(&c.driver_id)));
        }
        let origin = queue_item.origin_lat.zip(queue_item.origin_lng);
        let distance = |l: &crate::infrastructure::db::home_repo::HomeLead| match (origin, l.lat.zip(l.lng)) {
            (Some((a_lat, a_lng)), Some((b_lat, b_lng))) => ((a_lat - b_lat).powi(2) + (a_lng - b_lng).powi(2)).sqrt(),
            _ => f64::MAX,
        };
        leads.sort_by(|a, b| distance(a).partial_cmp(&distance(b)).unwrap_or(std::cmp::Ordering::Equal));
        leads.truncate(HOME_OFFER_MAX_LEADS);
        if leads.is_empty() {
            return Err(AppError::BusinessRule(
                "No home-move lead can take this move that day — it stays with ops".into(),
            ));
        }
        // The lead's net pay, after commission, as order-intake priced it —
        // shown on the card before anyone accepts. A would-be captain sees
        // the captain's part.
        let candidates: Vec<OfferCandidateRow> = leads
            .iter()
            .map(|l| OfferCandidateRow {
                driver_id: l.driver_id,
                payout_cents: if whole.contains(&l.driver_id) { req.lead_payout_cents } else { req.captain_payout_cents },
            })
            .collect();

        let offer = TaskOfferRow {
            id:                Uuid::new_v4(),
            tenant_id:         tenant_id.inner(),
            shipment_id:       queue_item.shipment_id,
            queue_id:          queue_item.id,
            status:            "open".into(),
            // The last wave: expiry returns it to ops, never to the geo pool.
            wave:              OFFER_MAX_WAVES,
            expires_at:        Utc::now() + Duration::seconds(HOME_OFFER_TTL_SECS),
            claimed_by:        None,
            merchant_name:     queue_item.merchant_name.clone(),
            // The card names the job: a whole move, or a joint mission.
            delivery_category: if crate::infrastructure::db::home_repo::joint_allowed(&req) { "home_move_joint" } else { "home_move" }.into(),
            weight_grams:      queue_item.weight_grams,
            tracking_number:   queue_item.tracking_number.clone().unwrap_or_default(),
            customer_name:     queue_item.customer_name.clone(),
            pickup_address:    format!("{}, {}", queue_item.origin_address_line1, queue_item.origin_city),
            delivery_address:  format!("{}, {}", queue_item.dest_address_line1, queue_item.dest_city),
            cod_amount_cents:  None,
            pickup_lat:        queue_item.origin_lat,
            pickup_lng:        queue_item.origin_lng,
            delivery_lat:      queue_item.dest_lat,
            delivery_lng:      queue_item.dest_lng,
        };
        self.offer_repo.create_offer(&offer, &candidates).await.map_err(AppError::Internal)?;
        home.mark_offer_slot(offer.id, offer.shipment_id, crate::infrastructure::db::home_repo::SLOT_PRIMARY)
            .await
            .map_err(AppError::Internal)?;
        self.publish_offer_created(&offer, &candidates).await;
        tracing::info!(
            offer_id = %offer.id, shipment_id = %queue_item.shipment_id, leads = candidates.len(),
            trucks = req.trucks, crew = req.crew_total, "Home move offered to leads"
        );
        Ok(offer)
    }

    /// Offer a side slot on a home move — a joint mission's Support Lead, or
    /// an addendum's extra truck — to the leads who can fill it: `count`
    /// offers, each for one truck and its crew, at `payout_cents` each. The
    /// move's queue row is left to its primary lead.
    pub async fn broadcast_side_slots(
        &self,
        tenant_id: TenantId,
        shipment_id: Uuid,
        slot: &str,
        count: usize,
        payout_cents: Option<i64>,
        ttl_secs: i64,
    ) -> AppResult<Vec<Uuid>> {
        use crate::infrastructure::db::home_repo::{move_date_of, SlotNeed, ROLE_EMERGENCY, ROLE_SUPPORT};
        let home = self.home.as_ref().ok_or_else(|| AppError::ServiceUnavailable("Home moves are not wired here".into()))?;
        let queue_item = self.queue_repo.find_by_shipment(shipment_id).await.map_err(AppError::Internal)?
            .filter(|q| q.tenant_id == tenant_id.inner())
            .ok_or_else(|| AppError::NotFound { resource: "Shipment in dispatch queue", id: shipment_id.to_string() })?;
        let req = home.requirement(shipment_id).await.map_err(AppError::Internal)?.ok_or_else(|| {
            AppError::BusinessRule("This home move has no crew requirement on record".into())
        })?;
        let on_it = home.reserved_drivers(shipment_id).await.map_err(AppError::Internal)?;
        let need = match slot {
            ROLE_SUPPORT => SlotNeed::support(&req, on_it),
            ROLE_EMERGENCY => SlotNeed::emergency(on_it),
            other => return Err(AppError::Validation(format!("\"{other}\" is not a side slot"))),
        };
        let leads = home
            .eligible_for(tenant_id.inner(), req.international, move_date_of(&req), &need)
            .await
            .map_err(AppError::Internal)?;
        if leads.is_empty() {
            tracing::warn!(%shipment_id, slot, "No lead can fill this home-move slot — it stays with ops");
            return Err(AppError::BusinessRule(format!("No lead can take the {slot} slot on this move — it stays with ops")));
        }
        let candidates: Vec<OfferCandidateRow> =
            leads.iter().map(|l| OfferCandidateRow { driver_id: l.driver_id, payout_cents }).collect();
        let mut offered = Vec::with_capacity(count);
        for _ in 0..count.max(1) {
            let offer = TaskOfferRow {
                id:                Uuid::new_v4(),
                tenant_id:         tenant_id.inner(),
                shipment_id,
                queue_id:          queue_item.id,
                status:            "open".into(),
                wave:              OFFER_MAX_WAVES,
                expires_at:        Utc::now() + Duration::seconds(ttl_secs),
                claimed_by:        None,
                merchant_name:     queue_item.merchant_name.clone(),
                // The card names the slot.
                delivery_category: format!("home_move_{slot}"),
                weight_grams:      queue_item.weight_grams,
                tracking_number:   queue_item.tracking_number.clone().unwrap_or_default(),
                customer_name:     queue_item.customer_name.clone(),
                pickup_address:    format!("{}, {}", queue_item.origin_address_line1, queue_item.origin_city),
                delivery_address:  format!("{}, {}", queue_item.dest_address_line1, queue_item.dest_city),
                cod_amount_cents:  None,
                pickup_lat:        queue_item.origin_lat,
                pickup_lng:        queue_item.origin_lng,
                delivery_lat:      queue_item.dest_lat,
                delivery_lng:      queue_item.dest_lng,
            };
            self.offer_repo.create_side_offer(&offer, &candidates).await.map_err(AppError::Internal)?;
            home.mark_offer_slot(offer.id, shipment_id, slot).await.map_err(AppError::Internal)?;
            self.publish_offer_created(&offer, &candidates).await;
            offered.push(offer.id);
        }
        tracing::info!(%shipment_id, slot, offers = offered.len(), leads = candidates.len(), "Home-move side slot offered");
        Ok(offered)
    }

    /// A lead's claim on a home offer: a reservation for the day, not an
    /// assignment — that comes twelve hours before the move.
    async fn claim_home(
        &self,
        home: &crate::infrastructure::db::HomeRepo,
        driver_id: &DriverId,
        offer_id: Uuid,
    ) -> AppResult<serde_json::Value> {
        use crate::infrastructure::db::ReserveOutcome;
        match home.reserve(offer_id, driver_id.inner()).await.map_err(AppError::Internal)? {
            ReserveOutcome::Reserved { tenant_id, shipment_id, move_date, all_candidate_ids, role } => {
                let closed = Event::new("dispatch", "offer.closed", tenant_id,
                    logisticos_events::payloads::TaskOfferClosed {
                        offer_id,
                        tenant_id,
                        shipment_id,
                        reason: "claimed".into(),
                        claimed_by: Some(driver_id.inner()),
                        candidate_driver_ids: all_candidate_ids,
                    });
                if let Err(e) = self.kafka.publish_event(topics::TASK_OFFER_CLOSED, &closed).await {
                    tracing::warn!(offer_id = %offer_id, err = %e, "home TaskOfferClosed publish failed (non-fatal)");
                }
                tracing::info!(offer_id = %offer_id, driver_id = %driver_id, %move_date, role = %role, "Home move reserved");
                // A Mission Captain's truck is one of two: offer the second.
                if role == crate::infrastructure::db::home_repo::ROLE_CAPTAIN {
                    let support_pay = home.requirement(shipment_id).await.ok().flatten().and_then(|r| r.support_payout_cents);
                    if let Err(e) = self
                        .broadcast_side_slots(TenantId::from_uuid(tenant_id), shipment_id, crate::infrastructure::db::home_repo::ROLE_SUPPORT, 1, support_pay, HOME_OFFER_TTL_SECS)
                        .await
                    {
                        tracing::warn!(%shipment_id, err = %e, "Joint mission: support slot not offered — ops must find the second truck");
                    }
                }
                Ok(serde_json::json!({
                    "reserved":      true,
                    "shipment_id":   shipment_id,
                    "move_date":     move_date,
                    "role":          role,
                    "assignment_id": null,
                }))
            }
            ReserveOutcome::AlreadyTaken | ReserveOutcome::Expired => Err(AppError::Conflict(ERR_OFFER_TAKEN.into())),
            ReserveOutcome::DayFull => Err(AppError::Conflict(ERR_DRIVER_BUSY.into())),
            ReserveOutcome::NotACandidate => Err(AppError::NotFound { resource: "Offer", id: offer_id.to_string() }),
        }
    }

    /// A cancelled shipment's open offers are gone: tell every candidate, so
    /// the card leaves their screen. Best-effort, as for an expiry.
    pub async fn announce_cancelled(&self, tenant_id: Uuid, shipment_id: Uuid, offers: &[(Uuid, Vec<Uuid>)]) {
        for (offer_id, candidates) in offers {
            let closed = Event::new("dispatch", "offer.closed", tenant_id,
                logisticos_events::payloads::TaskOfferClosed {
                    offer_id:    *offer_id,
                    tenant_id,
                    shipment_id,
                    reason:      "cancelled".into(),
                    claimed_by:  None,
                    candidate_driver_ids: candidates.clone(),
                });
            if let Err(e) = self.kafka.publish_event(topics::TASK_OFFER_CLOSED, &closed).await {
                tracing::warn!(offer_id = %offer_id, err = %e, "TaskOfferClosed(cancelled) publish failed (non-fatal)");
            }
        }
    }

    /// Wave radius for a 1-based wave number (clamped to the widest ring).
    pub fn radius_for_wave(wave: i32) -> f64 {
        let idx = (wave.max(1) as usize - 1).min(WAVE_RADIUS_KM.len() - 1);
        WAVE_RADIUS_KM[idx]
    }

    /// Ops-console action: broadcast a pending shipment to the gig pool.
    pub async fn broadcast(
        &self,
        tenant_id: TenantId,
        shipment_id: Uuid,
    ) -> AppResult<TaskOfferRow> {
        let queue_item = self.queue_repo
            .find_by_shipment(shipment_id).await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound {
                resource: "Shipment in dispatch queue",
                id: shipment_id.to_string(),
            })?;

        if queue_item.status != "pending" {
            return Err(AppError::BusinessRule(format!(
                "Shipment {} is already {} — cannot broadcast",
                shipment_id, queue_item.status
            )));
        }
        if queue_item.tenant_id != tenant_id.inner() {
            return Err(AppError::Forbidden { resource: "Shipment".into() });
        }

        if queue_item.service_type == "home_move" {
            let home = self.home.as_ref().ok_or_else(|| {
                AppError::ServiceUnavailable("Home moves are not wired in this dispatch".into())
            })?;
            return self.broadcast_home(home, tenant_id, &queue_item).await;
        }

        let candidates = self
            .select_candidates(&tenant_id, &queue_item, 1, &[])
            .await?;
        if candidates.is_empty() {
            return Err(AppError::BusinessRule(
                "No eligible gig drivers nearby — try quick dispatch or widen later".into(),
            ));
        }

        let offer = TaskOfferRow {
            id:                Uuid::new_v4(),
            tenant_id:         tenant_id.inner(),
            shipment_id,
            queue_id:          queue_item.id,
            status:            "open".into(),
            wave:              1,
            expires_at:        Utc::now() + Duration::seconds(OFFER_TTL_SECS),
            claimed_by:        None,
            merchant_name:     queue_item.merchant_name.clone(),
            delivery_category: queue_item.delivery_category.clone(),
            weight_grams:      queue_item.weight_grams,
            tracking_number:   queue_item.tracking_number.clone().unwrap_or_default(),
            customer_name:     queue_item.customer_name.clone(),
            pickup_address:    format!("{}, {}", queue_item.origin_address_line1, queue_item.origin_city),
            delivery_address:  format!("{}, {}", queue_item.dest_address_line1, queue_item.dest_city),
            cod_amount_cents:  queue_item.cod_amount_cents,
            pickup_lat:        queue_item.origin_lat,
            pickup_lng:        queue_item.origin_lng,
            delivery_lat:      queue_item.dest_lat,
            delivery_lng:      queue_item.dest_lng,
        };

        self.offer_repo.create_offer(&offer, &candidates).await
            .map_err(AppError::Internal)?;

        self.publish_offer_created(&offer, &candidates).await;

        tracing::info!(
            offer_id = %offer.id,
            shipment_id = %shipment_id,
            candidates = candidates.len(),
            "Gig offer broadcast (wave 1)"
        );
        Ok(offer)
    }

    /// The grab. Deterministic outcomes; post-commit event fan-out.
    pub async fn claim(&self, driver_id: &DriverId, offer_id: Uuid) -> AppResult<serde_json::Value> {
        if let Some(home) = &self.home {
            if home.offer_is_home(offer_id).await.map_err(AppError::Internal)? {
                return self.claim_home(home, driver_id, offer_id).await;
            }
        }
        let outcome = self.offer_repo.claim(offer_id, driver_id.inner()).await
            .map_err(AppError::Internal)?;

        match outcome {
            ClaimOutcome::Won {
                tenant_id, shipment_id, payout_cents,
                assignment_id, route_id, all_candidate_ids,
            } => {
                // Post-commit: emit the same assignment events as quick_dispatch,
                // then tell the losers. Fire-and-forget semantics on the loser
                // fan-out — a Kafka hiccup must not undo a won claim.
                if let Ok(Some(queue_item)) = self.queue_repo.find_by_shipment(shipment_id).await {
                    if let Err(e) = super::assignment_events::emit_assignment_events(
                        &self.kafka, &queue_item, tenant_id, driver_id.inner(),
                        assignment_id, route_id, shipment_id, payout_cents,
                    ).await {
                        tracing::error!(offer_id = %offer_id, err = %e,
                            "claim won but assignment event emission failed — driver-ops task rows missing until replay");
                    }
                }

                let closed = Event::new("dispatch", "offer.closed", tenant_id,
                    logisticos_events::payloads::TaskOfferClosed {
                        offer_id,
                        tenant_id,
                        shipment_id,
                        reason: "claimed".into(),
                        claimed_by: Some(driver_id.inner()),
                        candidate_driver_ids: all_candidate_ids,
                    });
                if let Err(e) = self.kafka.publish_event(topics::TASK_OFFER_CLOSED, &closed).await {
                    tracing::warn!(offer_id = %offer_id, err = %e,
                        "TaskOfferClosed publish failed (non-fatal — losers' cards expire by TTL)");
                }

                tracing::info!(offer_id = %offer_id, driver_id = %driver_id, "Offer claimed");
                Ok(serde_json::json!({
                    "assignment_id": assignment_id,
                    "shipment_id":   shipment_id,
                    "route_id":      route_id,
                    "payout_cents":  payout_cents,
                }))
            }
            // 409s — deterministic race outcomes, not validation failures.
            ClaimOutcome::AlreadyTaken | ClaimOutcome::Expired =>
                Err(AppError::Conflict(ERR_OFFER_TAKEN.into())),
            ClaimOutcome::DriverBusy =>
                Err(AppError::Conflict(ERR_DRIVER_BUSY.into())),
            ClaimOutcome::NotACandidate =>
                Err(AppError::NotFound { resource: "Offer", id: offer_id.to_string() }),
        }
    }

    pub async fn pass(&self, driver_id: &DriverId, offer_id: Uuid) -> AppResult<()> {
        self.offer_repo.record_pass(offer_id, driver_id.inner()).await
            .map_err(AppError::Internal)
    }

    pub async fn seen(&self, driver_id: &DriverId, offer_id: Uuid) -> AppResult<()> {
        self.offer_repo.mark_seen(offer_id, driver_id.inner()).await
            .map_err(AppError::Internal)
    }

    pub async fn open_for_driver(&self, driver_id: &DriverId) -> AppResult<Vec<OpenOfferView>> {
        self.offer_repo.find_open_for_driver(driver_id.inner()).await
            .map_err(AppError::Internal)
    }

    /// Sweeper tick (bootstrap interval): escalate or expire stale waves.
    pub async fn sweep(&self) -> anyhow::Result<()> {
        for offer in self.offer_repo.list_expired_open().await? {
            if let Err(e) = self.escalate_or_expire(&offer).await {
                tracing::warn!(offer_id = %offer.id, err = %e, "offer sweep: item failed (continuing)");
            }
        }
        Ok(())
    }

    async fn escalate_or_expire(&self, offer: &TaskOfferRow) -> anyhow::Result<()> {
        let tenant_id = TenantId::from_uuid(offer.tenant_id);

        if offer.wave < OFFER_MAX_WAVES {
            // Next wave: wider ring, fresh TTL, new candidates only (anyone
            // already offered keeps their row; passed drivers stay excluded).
            let exclude = self.offer_repo.candidate_ids(offer.id).await?;
            if let Ok(Some(queue_item)) = self.queue_repo.find_by_shipment(offer.shipment_id).await {
                let new_wave = offer.wave + 1;
                let fresh = self
                    .select_candidates(&tenant_id, &queue_item, new_wave, &exclude)
                    .await
                    .unwrap_or_default();
                if !fresh.is_empty() {
                    let new_expires = Utc::now() + Duration::seconds(OFFER_TTL_SECS);
                    if self.offer_repo
                        .escalate_wave(offer.id, new_wave, new_expires, &fresh)
                        .await?
                    {
                        let mut escalated = offer.clone();
                        escalated.wave = new_wave;
                        escalated.expires_at = new_expires;
                        self.publish_offer_created(&escalated, &fresh).await;
                        tracing::info!(
                            offer_id = %offer.id, wave = new_wave,
                            new_candidates = fresh.len(), "Gig offer escalated"
                        );
                        return Ok(());
                    }
                }
            }
            // No fresh candidates (or queue row gone) — fall through to expiry.
        }

        if let Some(candidate_ids) = self.offer_repo.expire(offer.id).await? {
            // Return the queue row to 'pending' so the standard dispatch sweep
            // picks it up and 1:1-assigns it (the fallback for unclaimed gig
            // work — full-time drivers, manual ops, etc). create_offer parked
            // it as 'dispatched' to dodge the sweep race; expiry undoes that.
            // A home move's side slot never parked it, so leaves it alone.
            let side_slot = match &self.home {
                Some(home) => home
                    .offer_slot(offer.id)
                    .await
                    .ok()
                    .flatten()
                    .is_some_and(|s| s != crate::infrastructure::db::home_repo::SLOT_PRIMARY),
                None => false,
            };
            if side_slot {
                tracing::warn!(offer_id = %offer.id, shipment_id = %offer.shipment_id, "Home-move side slot expired unfilled — ops must find the truck");
            } else {
                let _ = self.queue_repo.reset_to_pending(offer.shipment_id).await;
            }

            let closed = Event::new("dispatch", "offer.closed", offer.tenant_id,
                logisticos_events::payloads::TaskOfferClosed {
                    offer_id:    offer.id,
                    tenant_id:   offer.tenant_id,
                    shipment_id: offer.shipment_id,
                    reason:      "expired".into(),
                    claimed_by:  None,
                    candidate_driver_ids: candidate_ids,
                });
            if let Err(e) = self.kafka.publish_event(topics::TASK_OFFER_CLOSED, &closed).await {
                tracing::warn!(offer_id = %offer.id, err = %e,
                    "TaskOfferClosed(expired) publish failed (non-fatal)");
            }
            tracing::info!(offer_id = %offer.id, wave = offer.wave, "Gig offer expired unclaimed");
        }
        Ok(())
    }

    /// Candidate pipeline — identical gates to quick_dispatch (proximity →
    /// vehicle capacity → compliance) PLUS the gig filter: only part-time
    /// drivers (gig_rate_cents = Some) participate in broadcasts. Top N by
    /// the same 0.7·distance + 0.3·load score, payout snapshotted per driver.
    async fn select_candidates(
        &self,
        tenant_id: &TenantId,
        queue_item: &DispatchQueueRow,
        wave: i32,
        exclude: &[Uuid],
    ) -> AppResult<Vec<OfferCandidateRow>> {
        let anchor = match (
            queue_item.origin_lat.or(queue_item.dest_lat),
            queue_item.origin_lng.or(queue_item.dest_lng),
        ) {
            (Some(lat), Some(lng)) => Coordinates { lat, lng },
            _ => return Err(AppError::BusinessRule(
                "Shipment has no origin/destination coordinates — cannot broadcast. \
                 Ensure the merchant address is geocoded before booking.".into(),
            )),
        };

        let mut pool = self.driver_avail_repo
            .find_available_near(tenant_id, anchor, Self::radius_for_wave(wave))
            .await
            .map_err(AppError::Internal)?;

        // Passed this offer, or dropped this shipment before.
        let dropped = self.queue_repo
            .dropped_drivers(queue_item.shipment_id)
            .await
            .map_err(AppError::Internal)?;
        pool.retain(|c| !exclude.contains(&c.driver_id.inner()) && !dropped.contains(&c.driver_id.inner()));
        pool.retain(|c| vehicle_can_carry(
            c.vehicle_type.as_deref(),
            queue_item.weight_grams,
            &queue_item.delivery_category,
        ));

        // Rank exactly like quick_dispatch's auto-selection, then cap the wave.
        pool.sort_by(|a, b| {
            let sa = a.distance_km * 0.7 + a.active_stop_count as f64 * 0.3;
            let sb = b.distance_km * 0.7 + b.active_stop_count as f64 * 0.3;
            sa.partial_cmp(&sb).unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut candidates = Vec::with_capacity(OFFER_WAVE_SIZE);
        for c in pool {
            if candidates.len() >= OFFER_WAVE_SIZE {
                break;
            }
            // Gig filter + per-driver payout snapshot in one lookup.
            let rate = self.driver_avail_repo
                .gig_rate_cents(&c.driver_id).await
                .unwrap_or(None);
            let Some(rate) = rate else { continue };  // full-time → not in broadcasts

            // Compliance gate (same default-open semantics as quick_dispatch).
            if let Some(ref cc) = self.compliance_cache {
                let mut cache = cc.lock().await;
                let assignable = match cache.get_status(c.driver_id.inner()).await {
                    Ok(Some((_, assignable))) => assignable,
                    _ => true,
                };
                if !assignable {
                    continue;
                }
            }

            candidates.push(OfferCandidateRow {
                driver_id:    c.driver_id.inner(),
                payout_cents: Some(rate),
            });
        }
        Ok(candidates)
    }

    async fn publish_offer_created(&self, offer: &TaskOfferRow, candidates: &[OfferCandidateRow]) {
        let event = Event::new("dispatch", "offer.created", offer.tenant_id,
            logisticos_events::payloads::TaskOfferCreated {
                offer_id:          offer.id,
                tenant_id:         offer.tenant_id,
                shipment_id:       offer.shipment_id,
                wave:              offer.wave,
                expires_at:        offer.expires_at,
                candidates:        candidates.iter().map(|c| logisticos_events::payloads::OfferCandidate {
                    driver_id:    c.driver_id,
                    payout_cents: c.payout_cents,
                }).collect(),
                merchant_name:     offer.merchant_name.clone(),
                delivery_category: offer.delivery_category.clone(),
                weight_grams:      offer.weight_grams.max(0) as u32,
                tracking_number:   offer.tracking_number.clone(),
                customer_name:     offer.customer_name.clone(),
                pickup_address:    offer.pickup_address.clone(),
                delivery_address:  offer.delivery_address.clone(),
                cod_amount_cents:  offer.cod_amount_cents,
                pickup_lat:        offer.pickup_lat,
                pickup_lng:        offer.pickup_lng,
                delivery_lat:      offer.delivery_lat,
                delivery_lng:      offer.delivery_lng,
            });
        // Fire-and-forget: the DB row is the source of truth; the app's
        // GET /v1/offers/open recovery path covers a missed push.
        if let Err(e) = self.kafka.publish_event(topics::TASK_OFFER_CREATED, &event).await {
            tracing::warn!(offer_id = %offer.id, err = %e,
                "TaskOfferCreated publish failed — relying on app pull recovery");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wave_radius_widens_and_clamps() {
        assert_eq!(OfferService::radius_for_wave(1), 3.0);
        assert_eq!(OfferService::radius_for_wave(2), 6.0);
        assert_eq!(OfferService::radius_for_wave(3), 10.0);
        // Defensive: out-of-range waves clamp to the widest ring.
        assert_eq!(OfferService::radius_for_wave(0), 3.0);
        assert_eq!(OfferService::radius_for_wave(99), 10.0);
    }
}
