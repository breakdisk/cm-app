use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A configurable vehicle or sea-container spec defining interior cargo bay dimensions.
///
/// Covers road trucks (Van, L300, 6-Wheeler…) and sea containers (FCL20, FCL40).
/// Ops creates these via the admin portal; they are tenant-scoped.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TruckSpec {
    pub id:                 Uuid,
    pub tenant_id:          Uuid,
    /// Human-readable label, e.g. "6-Wheeler #3" or "20-ft Sea Container".
    pub name:               String,
    /// "road" | "sea" | "air"
    pub transport_mode:     String,
    /// Mirrors carrier.SizeClass: Van | L300 | 6Wheeler | 10Wheeler | Trailer | FCL20 | FCL40
    pub size_class:         String,
    /// Interior usable cargo bay — centimetres.
    pub interior_length_cm: i32,
    pub interior_width_cm:  i32,
    pub interior_height_cm: i32,
    /// Maximum payload capacity in kilograms.
    pub max_payload_kg:     f64,
    pub is_active:          bool,
    pub created_at:         DateTime<Utc>,
    pub updated_at:         DateTime<Utc>,
}

impl TruckSpec {
    pub fn volume_cm3(&self) -> i64 {
        self.interior_length_cm as i64
            * self.interior_width_cm as i64
            * self.interior_height_cm as i64
    }
}

/// The full packing plan for a batch of items against a specific truck spec.
///
/// `items`      — the input BoxItems (persisted so re-optimisation and page reload work).
/// `placements` — the algorithm output: X/Y/Z coordinates per placed item.
/// `unplaced`   — items that did not fit (reason: "no_space" or "weight_limit").
///
/// All three fields are stored as JSONB to avoid over-normalisation of what is
/// fundamentally a snapshot document, not relational data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidationPlan {
    pub id:               Uuid,
    pub tenant_id:        Uuid,
    pub hub_id:           Uuid,
    pub truck_spec_id:    Uuid,
    /// Linked once confirmed and physical loading begins. None = planning stage.
    pub container_id:     Option<Uuid>,
    /// Input BoxItems (serialised Vec<bin_pack::BoxItem>).
    pub items:            serde_json::Value,
    /// Output Placements (serialised Vec<bin_pack::Placement>).
    pub placements:       serde_json::Value,
    /// Unplaced items (serialised Vec<bin_pack::UnplacedItem>).
    pub unplaced:         serde_json::Value,
    pub total_weight_kg:  f64,
    pub volume_used_cm3:  i64,
    pub volume_total_cm3: i64,
    pub piece_count:      i32,
    pub computed_at:      DateTime<Utc>,
    pub created_at:       DateTime<Utc>,
    pub updated_at:       DateTime<Utc>,
}

impl ConsolidationPlan {
    pub fn volume_used_pct(&self) -> f32 {
        if self.volume_total_cm3 == 0 { return 0.0; }
        self.volume_used_cm3 as f32 / self.volume_total_cm3 as f32 * 100.0
    }

    pub fn weight_used_pct(&self, max_payload_kg: f64) -> f32 {
        if max_payload_kg == 0.0 { return 0.0; }
        (self.total_weight_kg / max_payload_kg * 100.0) as f32
    }
}
