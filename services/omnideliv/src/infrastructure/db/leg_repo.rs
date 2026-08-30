//! One leg, moved conditionally.
//!
//! Separate from `order_repo` on purpose. That writes a whole order as one unit
//! with `ON CONFLICT (id) DO UPDATE`, which is last-write-wins and correct for a
//! checkout: the order is the unit being written and nobody else is writing it.
//!
//! It is wrong for a transition two tablets may attempt at once. A kitchen has
//! a screen at the pass and a screen at the counter, and both will be tapped.
//! Through `save()` both would succeed and the second would silently overwrite
//! the first; here the loser learns it lost and is told the leg's real state.
//!
//! Keeping these in one file is how the guarded write eventually gets
//! "simplified" back into `save()` by someone who sees two ways to write a leg.

use async_trait::async_trait;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::domain::entities::LegStatus;
use crate::domain::repositories::{
    AwaitingLeg, LegTransition, TransitionResponse, VendorLegRepository, VendorLegRow,
};

/// The statuses a queue read returns. A leg outside this set is history: the
/// courier has it, the store refused it, or it is settled.
const LIVE_STATUSES: [&str; 4] = ["pending", "accepted", "preparing", "ready"];

pub struct PgVendorLegRepository {
    pool: PgPool,
}

impl PgVendorLegRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl VendorLegRepository for PgVendorLegRepository {
    async fn transition(
        &self,
        tenant_id:        Uuid,
        vendor_id:        Uuid,
        leg_id:           Uuid,
        to:               LegStatus,
        ready_in_minutes: Option<i32>,
        rejected_reason:  Option<&str>,
    ) -> anyhow::Result<LegTransition> {
        // Derived from the domain graph, never hand-written here. A change to
        // `can_transition_to` reaches this SQL automatically, so the rule
        // cannot be restated differently in two places.
        let from_strs: Vec<String> = LegStatus::ALL
            .iter()
            .filter(|s| s.can_transition_to(to))
            .map(|s| s.as_str().to_owned())
            .collect();

        // A target nothing can reach is a programming error, not a lost race —
        // without this it would present as a confusing `NoOp` on every attempt.
        if from_strs.is_empty() {
            anyhow::bail!("no legal predecessor for leg status {}", to.as_str());
        }

        // The whole guard is the WHERE clause. If another tablet already moved
        // this leg, `status = ANY($4)` no longer holds and zero rows update.
        // The timestamps are set here rather than by the caller so a retry
        // cannot rewrite when a store actually accepted.
        //
        // RETURNING rather than a follow-up SELECT: the caller publishes an
        // event straight after this and needs the order it belongs to. Reading
        // it back separately would also be a read of a row another writer may
        // have moved in between.
        let applied = sqlx::query(
            r#"
            UPDATE omnideliv.order_vendor_legs
               SET status           = $5,
                   accepted_at      = CASE WHEN $5 = 'accepted'  THEN NOW() ELSE accepted_at  END,
                   ready_at         = CASE WHEN $5 = 'ready'     THEN NOW() ELSE ready_at     END,
                   picked_up_at     = CASE WHEN $5 = 'picked_up' THEN NOW() ELSE picked_up_at END,
                   ready_in_minutes = COALESCE($6, ready_in_minutes),
                   rejected_reason  = COALESCE($7, rejected_reason)
             WHERE id        = $1
               AND tenant_id = $2
               AND vendor_id = $3
               AND status    = ANY($4)
         RETURNING order_id, goods_subtotal_cents
            "#,
        )
        .bind(leg_id)
        .bind(tenant_id)
        .bind(vendor_id)
        .bind(&from_strs)
        .bind(to.as_str())
        .bind(ready_in_minutes)
        .bind(rejected_reason)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(r) = applied {
            return Ok(LegTransition::Applied {
                to,
                order_id:             r.get("order_id"),
                goods_subtotal_cents: r.get("goods_subtotal_cents"),
            });
        }

        // Zero rows means one of two things and the caller must tell them
        // apart: the leg moved already (report where it actually is), or it is
        // not this vendor's leg at all (report nothing, and let the handler
        // 404 rather than confirm the id exists). Re-reading under the same
        // tenant+vendor scope answers both without widening the scope.
        let current: Option<String> = sqlx::query(
            r#"
            SELECT status FROM omnideliv.order_vendor_legs
             WHERE id = $1 AND tenant_id = $2 AND vendor_id = $3
            "#,
        )
        .bind(leg_id)
        .bind(tenant_id)
        .bind(vendor_id)
        .fetch_optional(&self.pool)
        .await?
        .map(|r| r.get::<String, _>("status"));

        match current {
            Some(s) => {
                let parsed = LegStatus::from_wire(&s)
                    .ok_or_else(|| anyhow::anyhow!("unknown leg status in database: {s}"))?;
                Ok(LegTransition::NoOp { current: parsed })
            }
            None => anyhow::bail!("leg {leg_id} not found for this vendor"),
        }
    }

    async fn list_open(
        &self,
        tenant_id: Uuid,
        vendor_id: Uuid,
    ) -> anyhow::Result<Vec<VendorLegRow>> {
        let rows = sqlx::query(
            r#"
            SELECT id, order_id, status, goods_subtotal_cents,
                   ready_in_minutes, accepted_at, created_at
              FROM omnideliv.order_vendor_legs
             WHERE tenant_id = $1
               AND vendor_id = $2
               AND status    = ANY($3)
             ORDER BY created_at ASC
            "#,
        )
        .bind(tenant_id)
        .bind(vendor_id)
        .bind(&LIVE_STATUSES[..])
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| VendorLegRow {
                leg_id:               r.get("id"),
                order_id:             r.get("order_id"),
                status:               r.get("status"),
                goods_subtotal_cents: r.get("goods_subtotal_cents"),
                ready_in_minutes:     r.get("ready_in_minutes"),
                accepted_at:          r.get("accepted_at"),
                created_at:           r.get("created_at"),
            })
            .collect())
    }

    async fn find_awaiting_acceptance(&self) -> anyhow::Result<Vec<AwaitingLeg>> {
        // Bounded like `find_awaiting_courier`: a sweep that returns everything
        // turns one bad hour into an unbounded query. Oldest first, so the legs
        // nearest escalation are still handled when the cap bites.
        let rows = sqlx::query(
            r#"
            SELECT id, order_id, tenant_id, vendor_id, goods_subtotal_cents, created_at
              FROM omnideliv.order_vendor_legs
             WHERE status = 'pending'
             ORDER BY created_at ASC
             LIMIT 500
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| AwaitingLeg {
                leg_id:               r.get("id"),
                order_id:             r.get("order_id"),
                tenant_id:            r.get("tenant_id"),
                vendor_id:            r.get("vendor_id"),
                goods_subtotal_cents: r.get("goods_subtotal_cents"),
                created_at:           r.get("created_at"),
            })
            .collect())
    }

    async fn find_idempotent_response(
        &self,
        tenant_id: Uuid,
        vendor_id: Uuid,
        key:       &str,
    ) -> anyhow::Result<Option<TransitionResponse>> {
        let row = sqlx::query(
            r#"
            SELECT response FROM omnideliv.vendor_action_idempotency
             WHERE tenant_id = $1 AND vendor_id = $2 AND key = $3
            "#,
        )
        .bind(tenant_id)
        .bind(vendor_id)
        .bind(key)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(r) => Ok(Some(serde_json::from_value(
                r.get::<serde_json::Value, _>("response"),
            )?)),
            None => Ok(None),
        }
    }

    async fn record_idempotent_response(
        &self,
        tenant_id: Uuid,
        vendor_id: Uuid,
        key:       &str,
        leg_id:    Uuid,
        action:    &str,
        response:  &TransitionResponse,
    ) -> anyhow::Result<()> {
        // DO NOTHING rather than DO UPDATE: the first response for a key is the
        // answer that key gets forever. Overwriting it would let a retry return
        // a different result from the request it is supposedly replaying.
        sqlx::query(
            r#"
            INSERT INTO omnideliv.vendor_action_idempotency
                (tenant_id, vendor_id, key, leg_id, action, response)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (tenant_id, vendor_id, key) DO NOTHING
            "#,
        )
        .bind(tenant_id)
        .bind(vendor_id)
        .bind(key)
        .bind(leg_id)
        .bind(action)
        .bind(serde_json::to_value(response)?)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The SQL predecessor list is derived, so a drift between the domain graph
    /// and what the database will accept is the failure mode worth pinning.
    fn predecessors_of(to: LegStatus) -> Vec<&'static str> {
        LegStatus::ALL
            .iter()
            .filter(|s| s.can_transition_to(to))
            .map(|s| s.as_str())
            .collect()
    }

    #[test]
    fn accepting_is_reachable_only_from_pending() {
        assert_eq!(predecessors_of(LegStatus::Accepted), vec!["pending"]);
    }

    #[test]
    fn rejecting_is_reachable_only_from_pending() {
        // Once a store has accepted, it has committed. Backing out is an
        // operator action (`failed`), not a second bite at rejection.
        assert_eq!(predecessors_of(LegStatus::Rejected), vec!["pending"]);
    }

    #[test]
    fn ready_is_reachable_from_accepted_or_preparing() {
        assert_eq!(predecessors_of(LegStatus::Ready), vec!["accepted", "preparing"]);
    }

    #[test]
    fn served_is_reachable_only_from_ready() {
        assert_eq!(predecessors_of(LegStatus::Served), vec!["ready"]);
    }

    #[test]
    fn every_live_status_can_reach_failed() {
        assert_eq!(
            predecessors_of(LegStatus::Failed),
            vec!["pending", "accepted", "preparing", "ready", "picked_up", "served"],
        );
    }

    #[test]
    fn no_target_is_unreachable() {
        // An empty predecessor list makes `transition` bail. Only `Pending` is
        // legitimately unreachable — it is where a leg starts.
        for to in LegStatus::ALL {
            let n = predecessors_of(to).len();
            if to == LegStatus::Pending {
                assert_eq!(n, 0, "nothing should transition back into pending");
            } else {
                assert!(n > 0, "{to:?} is unreachable — transition would always bail");
            }
        }
    }

    #[test]
    fn the_live_status_list_matches_the_domain_predicate() {
        // `LIVE_STATUSES` is what the queue query filters on. If it drifts from
        // `blocks_collection`, the queue and the order state machine disagree
        // about which legs are outstanding.
        let from_domain: Vec<&str> = LegStatus::ALL
            .iter()
            .filter(|s| s.blocks_collection())
            .map(|s| s.as_str())
            .collect();
        assert_eq!(from_domain, LIVE_STATUSES.to_vec());
    }
}
