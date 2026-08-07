//! Courier assignment and its claim lifecycle.
//!
//! `product` and `external_ref` are deliberately opaque: field-ops must not
//! interpret a product's job id, or the tier stops being product-agnostic.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Which product owns an assignment.
///
/// An opaque key, not an enum. A Rust enum here would be the same closed set
/// the rejected `CHECK (product IN (...))` was — admitting a third consumer
/// would mean editing this tier's source and redeploying it, which is exactly
/// the property ADR-0015 says disqualifies something from being a platform
/// tier. The registry table `field_ops.products` is the authority; the FK on
/// `courier_assignments.product` is what rejects an unknown key, at the same
/// moment a CHECK would have, without naming the consumers in code.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProductKey(String);

impl ProductKey {
    pub fn new(key: impl Into<String>) -> Self { Self(key.into()) }
    pub fn as_str(&self) -> &str { &self.0 }
}

impl std::fmt::Display for ProductKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssignmentStatus {
    Offered,
    Claimed,
    Completed,
    Released,
    Expired,
}

impl AssignmentStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            AssignmentStatus::Offered   => "offered",
            AssignmentStatus::Claimed   => "claimed",
            AssignmentStatus::Completed => "completed",
            AssignmentStatus::Released  => "released",
            AssignmentStatus::Expired   => "expired",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CourierAssignment {
    pub id:           Uuid,
    pub tenant_id:    Uuid,
    pub courier_id:   Uuid,
    pub product:      ProductKey,
    pub external_ref: Uuid,
    pub status:       AssignmentStatus,
    pub offered_at:   DateTime<Utc>,
    pub claimed_at:   Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub heartbeat_at: Option<DateTime<Utc>>,
    pub created_at:   DateTime<Utc>,
}

impl CourierAssignment {
    pub fn offer(tenant_id: Uuid, courier_id: Uuid, product: ProductKey, external_ref: Uuid) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            courier_id,
            product,
            external_ref,
            status: AssignmentStatus::Offered,
            offered_at: now,
            claimed_at: None,
            completed_at: None,
            heartbeat_at: None,
            created_at: now,
        }
    }

    pub fn claim(&mut self) {
        let now = Utc::now();
        self.status = AssignmentStatus::Claimed;
        self.claimed_at = Some(now);
        self.heartbeat_at = Some(now);
    }

    pub fn complete(&mut self) {
        self.status = AssignmentStatus::Completed;
        self.completed_at = Some(Utc::now());
    }

    pub fn release(&mut self) {
        self.status = AssignmentStatus::Released;
    }

    pub fn heartbeat(&mut self) {
        self.heartbeat_at = Some(Utc::now());
    }

    /// Does this assignment currently tie up its courier? Only a live claim does.
    pub fn holds_courier(&self) -> bool {
        self.status == AssignmentStatus::Claimed
    }

    /// A claimed assignment whose heartbeat has gone quiet past the TTL. Such a
    /// claim is reclaimable — a crashed client must not hold a courier forever.
    pub fn is_stale(&self, ttl_secs: i64) -> bool {
        if self.status != AssignmentStatus::Claimed {
            return false;
        }
        match self.heartbeat_at {
            None => true,
            Some(hb) => Utc::now() - hb > Duration::seconds(ttl_secs),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn offered() -> CourierAssignment {
        CourierAssignment::offer(Uuid::new_v4(), Uuid::new_v4(), ProductKey::new("omnideliv"), Uuid::new_v4())
    }

    #[test]
    fn a_new_assignment_is_offered_not_claimed() {
        let a = offered();
        assert_eq!(a.status, AssignmentStatus::Offered);
        assert!(a.claimed_at.is_none());
        assert!(!a.holds_courier());
    }

    #[test]
    fn claiming_marks_it_claimed_and_starts_the_heartbeat() {
        let mut a = offered();
        a.claim();
        assert_eq!(a.status, AssignmentStatus::Claimed);
        assert!(a.claimed_at.is_some());
        assert!(a.heartbeat_at.is_some(), "a claim must start its heartbeat or it is immediately stale");
        assert!(a.holds_courier());
    }

    /// Only a claimed assignment ties up a courier. Completed, released and
    /// expired ones must not — otherwise a courier is never reusable.
    #[test]
    fn only_a_claimed_assignment_holds_the_courier() {
        for terminal in [AssignmentStatus::Completed, AssignmentStatus::Released, AssignmentStatus::Expired] {
            let mut a = offered();
            a.claim();
            a.status = terminal;
            assert!(!a.holds_courier(), "{terminal:?} must not hold the courier");
        }
    }

    #[test]
    fn a_claim_is_stale_once_the_heartbeat_window_passes() {
        let mut a = offered();
        a.claim();
        assert!(!a.is_stale(120), "a fresh claim is not stale");

        a.heartbeat_at = Some(chrono::Utc::now() - chrono::Duration::seconds(300));
        assert!(a.is_stale(120), "a claim with a 5-minute-old heartbeat is stale at a 120s TTL");
    }

    /// Only a *claimed* assignment can be stale. An offered one has no
    /// heartbeat yet, and treating that as stale would make the reclaim sweep
    /// expire assignments it never handed out.
    #[test]
    fn a_never_claimed_assignment_is_not_stale() {
        let a = offered();
        assert!(a.heartbeat_at.is_none());
        assert!(!a.is_stale(120));
    }

    /// The product key is opaque and round-trips verbatim — no normalisation,
    /// no enum narrowing. An unknown key is the database's FK to reject, which
    /// is what keeps a third consumer from needing a code change here.
    #[test]
    fn an_unknown_product_key_is_carried_without_complaint() {
        let a = CourierAssignment::offer(
            Uuid::new_v4(), Uuid::new_v4(), ProductKey::new("ride_hailing"), Uuid::new_v4());
        assert_eq!(a.product.as_str(), "ride_hailing");
    }
}
