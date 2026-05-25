use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

const CHANNEL_CAPACITY: usize = 64;

/// Per-hub WebSocket broadcast bus.
///
/// Consolidation events (plan_computed, box_scanned) are published here;
/// the WS handler forwards them to all subscribed browser sessions for that hub.
#[derive(Clone)]
pub struct HubBroadcaster {
    inner: Arc<RwLock<HashMap<Uuid, broadcast::Sender<String>>>>,
}

impl HubBroadcaster {
    pub fn new() -> Self {
        Self { inner: Arc::new(RwLock::new(HashMap::new())) }
    }

    /// Subscribe to events for `hub_id`. Creates the channel on first subscription.
    pub async fn subscribe(&self, hub_id: Uuid) -> broadcast::Receiver<String> {
        {
            let guard = self.inner.read().await;
            if let Some(tx) = guard.get(&hub_id) {
                return tx.subscribe();
            }
        }
        let mut guard = self.inner.write().await;
        // Re-check after acquiring write lock.
        let tx = guard
            .entry(hub_id)
            .or_insert_with(|| broadcast::channel(CHANNEL_CAPACITY).0);
        tx.subscribe()
    }

    /// Broadcast a JSON string to all subscribers for `hub_id`.
    /// Silently drops if no subscribers.
    pub async fn broadcast(&self, hub_id: Uuid, message: String) {
        let guard = self.inner.read().await;
        if let Some(tx) = guard.get(&hub_id) {
            let _ = tx.send(message);
        }
    }
}

impl Default for HubBroadcaster {
    fn default() -> Self { Self::new() }
}
