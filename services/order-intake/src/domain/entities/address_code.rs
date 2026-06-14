use serde::Serialize;

/// A single entry from the global `reference.address_codes` lookup table.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AddressCode {
    pub country_code:   String,
    pub postal_code:    String,
    pub city:           String,
    pub state_province: Option<String>,
    pub region:         Option<String>,
    pub time_zone:      Option<String>,
}
