use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use logisticos_types::TenantId;

/// Stable internal identifier for a CDP customer profile.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CustomerId(Uuid);

impl CustomerId {
    pub fn new() -> Self { Self(Uuid::new_v4()) }
    pub fn from_uuid(id: Uuid) -> Self { Self(id) }
    pub fn inner(&self) -> Uuid { self.0 }
}

impl Default for CustomerId {
    fn default() -> Self { Self::new() }
}

// ---------------------------------------------------------------------------
// Behavioral event — immutable record of something the customer did / had done.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    ShipmentCreated,
    ShipmentConfirmed,
    DeliveryAttempted,
    DeliveryCompleted,
    DeliveryFailed,
    CodPaid,
    SupportTicketOpened,
    NotificationRead,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehavioralEvent {
    pub id:          Uuid,
    pub event_type:  EventType,
    pub shipment_id: Option<Uuid>,
    pub metadata:    serde_json::Value,
    pub occurred_at: DateTime<Utc>,
}

impl BehavioralEvent {
    pub fn new(
        event_type: EventType,
        shipment_id: Option<Uuid>,
        metadata: serde_json::Value,
        occurred_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            event_type,
            shipment_id,
            metadata,
            occurred_at,
        }
    }
}

// ---------------------------------------------------------------------------
// Address preference — most-used destination addresses.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddressUsage {
    pub address:    String,
    pub use_count:  u32,
    pub last_used:  DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// CustomerProfile — the core aggregate.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomerProfile {
    pub id:                     CustomerId,
    pub tenant_id:              TenantId,

    /// External ID — matches the customer_id used in the order-intake / shipments domain.
    pub external_customer_id:   Uuid,

    // Identity
    pub name:                   Option<String>,
    pub email:                  Option<String>,
    pub phone:                  Option<String>,

    // Delivery history (denormalised counters for fast read)
    pub total_shipments:        u32,
    pub successful_deliveries:  u32,
    pub failed_deliveries:      u32,
    pub first_shipment_at:      Option<DateTime<Utc>>,
    pub last_shipment_at:       Option<DateTime<Utc>>,

    // COD aggregate
    pub total_cod_collected_cents: i64,

    // Address intelligence
    pub address_history:        Vec<AddressUsage>,   // JSONB in DB

    // Behavioral timeline (last 90 events, JSONB array)
    pub recent_events:          Vec<BehavioralEvent>,

    // CLV and engagement score (0-100)
    pub clv_score:              f32,
    pub engagement_score:       f32,

    pub created_at:             DateTime<Utc>,
    pub updated_at:             DateTime<Utc>,
}

impl CustomerProfile {
    pub fn new(tenant_id: TenantId, external_customer_id: Uuid) -> Self {
        let now = Utc::now();
        Self {
            id:                        CustomerId::new(),
            tenant_id,
            external_customer_id,
            name:                      None,
            email:                     None,
            phone:                     None,
            total_shipments:           0,
            successful_deliveries:     0,
            failed_deliveries:         0,
            first_shipment_at:         None,
            last_shipment_at:          None,
            total_cod_collected_cents: 0,
            address_history:           Vec::new(),
            recent_events:             Vec::new(),
            clv_score:                 0.0,
            engagement_score:          0.0,
            created_at:                now,
            updated_at:                now,
        }
    }

    // ------------------------------------------------------------------
    // Identity enrichment
    // ------------------------------------------------------------------

    pub fn enrich_identity(
        &mut self,
        name: Option<String>,
        email: Option<String>,
        phone: Option<String>,
    ) {
        if name.is_some() { self.name = name; }
        if email.is_some() { self.email = email; }
        if phone.is_some() { self.phone = phone; }
        self.updated_at = Utc::now();
    }

    // ------------------------------------------------------------------
    // Behavioral event recording
    // ------------------------------------------------------------------

    pub fn record_event(&mut self, event: BehavioralEvent) {
        // Update counters based on event type.
        match &event.event_type {
            EventType::ShipmentCreated | EventType::ShipmentConfirmed => {
                self.total_shipments += 1;
                let now = event.occurred_at;
                if self.first_shipment_at.is_none() {
                    self.first_shipment_at = Some(now);
                }
                self.last_shipment_at = Some(now);
            }
            EventType::DeliveryCompleted => {
                self.successful_deliveries += 1;
            }
            EventType::DeliveryFailed => {
                self.failed_deliveries += 1;
            }
            EventType::CodPaid => {
                let amount = event.metadata
                    .get("amount_cents")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                self.total_cod_collected_cents += amount;
            }
            _ => {}
        }

        // Update address history if destination address is present.
        if let Some(addr) = event.metadata.get("destination_address").and_then(|v| v.as_str()) {
            self.record_address(addr.to_owned(), event.occurred_at);
        }

        // Keep most recent 90 events.
        self.recent_events.push(event);
        if self.recent_events.len() > 90 {
            self.recent_events.remove(0);
        }

        self.recalculate_scores();
        self.updated_at = Utc::now();
    }

    // ------------------------------------------------------------------
    // Address intelligence
    // ------------------------------------------------------------------

    fn record_address(&mut self, address: String, timestamp: DateTime<Utc>) {
        if let Some(existing) = self.address_history.iter_mut().find(|a| a.address == address) {
            existing.use_count += 1;
            existing.last_used = timestamp;
        } else {
            self.address_history.push(AddressUsage {
                address,
                use_count: 1,
                last_used: timestamp,
            });
        }
        // Keep top 20 addresses only (by use_count).
        if self.address_history.len() > 20 {
            self.address_history.sort_by(|a, b| b.use_count.cmp(&a.use_count));
            self.address_history.truncate(20);
        }
    }

    pub fn preferred_address(&self) -> Option<&str> {
        self.address_history
            .iter()
            .max_by_key(|a| a.use_count)
            .map(|a| a.address.as_str())
    }

    // ------------------------------------------------------------------
    // Score computation
    // ------------------------------------------------------------------

    /// CLV score: simple heuristic combining delivery volume, COD value, and recency.
    /// Range 0.0 – 100.0.
    fn recalculate_scores(&mut self) {
        let delivery_score = (self.successful_deliveries as f32 * 3.0).min(50.0);
        let cod_score = ((self.total_cod_collected_cents as f32) / 1_000_000.0 * 10.0).min(30.0);
        let recency_score = self.last_shipment_at.map(|t| {
            let days_since = (Utc::now() - t).num_days() as f32;
            20.0_f32 * (1.0 - (days_since / 365.0).min(1.0))
        }).unwrap_or(0.0);
        self.clv_score = (delivery_score + cod_score + recency_score).min(100.0);

        // Engagement score: recent activity in last 30 days.
        let thirty_days_ago = Utc::now() - chrono::Duration::days(30);
        let recent_count = self.recent_events
            .iter()
            .filter(|e| e.occurred_at > thirty_days_ago)
            .count() as f32;
        self.engagement_score = (recent_count * 10.0).min(100.0);
    }

    // ------------------------------------------------------------------
    // Delivery success rate
    // ------------------------------------------------------------------

    pub fn delivery_success_rate(&self) -> f32 {
        let total_deliveries = self.successful_deliveries + self.failed_deliveries;
        if total_deliveries == 0 {
            return 0.0;
        }
        self.successful_deliveries as f32 / total_deliveries as f32 * 100.0
    }
}
