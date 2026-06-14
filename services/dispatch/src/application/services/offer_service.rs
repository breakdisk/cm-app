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

/// Error markers the HTTP layer maps to typed 409s.
pub const ERR_OFFER_TAKEN: &str = "OFFER_TAKEN";
pub const ERR_DRIVER_BUSY: &str = "DRIVER_BUSY";

pub struct OfferService {
    offer_repo:        Arc<PgTaskOfferRepository>,
    queue_repo:        Arc<dyn DispatchQueueRepository>,
    driver_avail_repo: Arc<dyn DriverAvailabilityRepository>,
    kafka:             Arc<KafkaProducer>,
    compliance_cache:  Option<Arc<Mutex<ComplianceCache>>>,
}

impl OfferService {
    pub fn new(
        offer_repo:        Arc<PgTaskOfferRepository>,
        queue_repo:        Arc<dyn DispatchQueueRepository>,
        driver_avail_repo: Arc<dyn DriverAvailabilityRepository>,
        kafka:             Arc<KafkaProducer>,
        compliance_cache:  Option<Arc<Mutex<ComplianceCache>>>,
    ) -> Self {
        Self { offer_repo, queue_repo, driver_avail_repo, kafka, compliance_cache }
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
            .map_err(|e| AppError::Internal(e.into()))?
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
            let _ = self.queue_repo.reset_to_pending(offer.shipment_id).await;

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

        pool.retain(|c| !exclude.contains(&c.driver_id.inner()));
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
