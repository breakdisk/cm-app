//! Per-shipment failed-delivery counter behind the `DeliveryAttempted` trigger.
//!
//! No upstream event carries a running attempt count, so business-logic keeps
//! its own. Counting is keyed by `(tenant, shipment)` and idempotent on the
//! envelope event id, because Kafka redelivers.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use uuid::Uuid;

#[async_trait::async_trait]
pub trait AttemptStore: Send + Sync {
    /// Record one failed delivery attempt.
    ///
    /// Returns `Some(n)` (this was the shipment's `n`th attempt) when the
    /// event was new, and `None` when `event_id` was already recorded for the
    /// tenant (a redelivery), in which case nothing is counted.
    async fn record_attempt(
        &self,
        tenant_id: Uuid,
        shipment_id: Uuid,
        event_id: Uuid,
    ) -> anyhow::Result<Option<u32>>;
}

/// Process-local store. Used by tests; production uses `PgAttemptStore`.
#[derive(Default)]
pub struct InMemoryAttemptStore {
    state: Mutex<InMemoryState>,
}

#[derive(Default)]
struct InMemoryState {
    seen: HashSet<(Uuid, Uuid)>,
    counts: HashMap<(Uuid, Uuid), u32>,
}

#[async_trait::async_trait]
impl AttemptStore for InMemoryAttemptStore {
    async fn record_attempt(
        &self,
        tenant_id: Uuid,
        shipment_id: Uuid,
        event_id: Uuid,
    ) -> anyhow::Result<Option<u32>> {
        let mut st = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("attempt store mutex poisoned"))?;
        if !st.seen.insert((tenant_id, event_id)) {
            return Ok(None);
        }
        let n = st.counts.entry((tenant_id, shipment_id)).or_insert(0);
        *n += 1;
        Ok(Some(*n))
    }
}
