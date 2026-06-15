use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use logisticos_types::TenantId;

// ---------------------------------------------------------------------------
// SegmentFilter — the DSL stored as JSONB in cdp.segments.filter_criteria
// ---------------------------------------------------------------------------

/// All fields are optional; unset fields are treated as "no constraint".
/// Combined with AND semantics (customer must satisfy ALL set conditions).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SegmentFilter {
    /// Minimum CLV score (0-100).
    pub min_clv: Option<f32>,
    /// Maximum CLV score (0-100).
    pub max_clv: Option<f32>,
    /// Minimum engagement score (0-100).
    pub min_engagement: Option<f32>,
    /// Churn tier must match exactly: "high", "medium", "low", "healthy".
    pub churn_tier: Option<String>,
    /// Maximum churn score (0.0-1.0); customers at or below qualify.
    pub max_churn_score: Option<f32>,
    /// Customer must have had no shipment in the last N days.
    pub inactive_days: Option<i32>,
    /// Profile classification: "sender", "receiver", "unknown".
    pub profile_type: Option<String>,
    /// Minimum total successful deliveries.
    pub min_shipments: Option<i32>,
}

// ---------------------------------------------------------------------------
// Segment — the aggregate
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    pub id:              Uuid,
    pub tenant_id:       TenantId,
    pub name:            String,
    pub description:     String,
    /// Parsed from JSONB filter_criteria column.
    pub filter:          SegmentFilter,
    /// Cached member count — refreshed asynchronously; preview endpoints compute live.
    pub customer_count:  i32,
    pub is_dynamic:      bool,
    pub last_computed:   Option<DateTime<Utc>>,
    pub created_at:      DateTime<Utc>,
    pub updated_at:      DateTime<Utc>,
}

impl Segment {
    pub fn new(tenant_id: TenantId, name: String, description: String, filter: SegmentFilter) -> Self {
        let now = Utc::now();
        Self {
            id:             Uuid::new_v4(),
            tenant_id,
            name,
            description,
            filter,
            customer_count: 0,
            is_dynamic:     true,
            last_computed:  None,
            created_at:     now,
            updated_at:     now,
        }
    }
}

// ---------------------------------------------------------------------------
// SegmentMember — lightweight row returned for member listings
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentMember {
    pub external_customer_id: Uuid,
    pub name:                 Option<String>,
    pub email:                Option<String>,
    pub phone:                Option<String>,
    pub clv_score:            f32,
    pub engagement_score:     f32,
    pub last_shipment_at:     Option<DateTime<Utc>>,
}
