use chrono::{DateTime, Utc};
use logisticos_types::{TenantId, UserId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PickupAddress {
    pub id:          uuid::Uuid,
    pub user_id:     UserId,
    pub tenant_id:   TenantId,
    pub label:       String,
    pub address:     String,
    pub city:        String,
    pub province:    String,
    pub postal_code: String,
    pub country:     String,
    pub lat:         Option<f64>,
    pub lng:         Option<f64>,
    pub is_default:  bool,
    pub created_at:  DateTime<Utc>,
    pub updated_at:  DateTime<Utc>,
}

impl PickupAddress {
    pub fn new(
        user_id: UserId,
        tenant_id: TenantId,
        label: String,
        address: String,
        city: String,
        province: String,
        postal_code: String,
        country: String,
        lat: Option<f64>,
        lng: Option<f64>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: uuid::Uuid::new_v4(),
            user_id,
            tenant_id,
            label,
            address,
            city,
            province,
            postal_code,
            country,
            lat,
            lng,
            is_default: false,
            created_at: now,
            updated_at: now,
        }
    }
}
