//! A business that supplies goods. Named `Vendor`, never `merchant` — see the
//! naming note in migration 0001.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Vertical {
    Restaurant,
    Grocery,
    Pharmacy,
    Florist,
    Retail,
}

impl Vertical {
    pub fn as_str(&self) -> &'static str {
        match self {
            Vertical::Restaurant => "restaurant",
            Vertical::Grocery    => "grocery",
            Vertical::Pharmacy   => "pharmacy",
            Vertical::Florist    => "florist",
            Vertical::Retail     => "retail",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VendorStatus {
    Onboarding,
    Active,
    Paused,
    Offboarded,
}

impl VendorStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            VendorStatus::Onboarding => "onboarding",
            VendorStatus::Active     => "active",
            VendorStatus::Paused     => "paused",
            VendorStatus::Offboarded => "offboarded",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vendor {
    pub id:                Uuid,
    pub tenant_id:         Uuid,
    pub vertical:          Vertical,
    pub name:              String,
    pub address:           String,
    pub lat:               f64,
    pub lng:               f64,
    pub prep_time_minutes: i32,
    pub commission_bps:    i32,
    pub payout_account:    Option<String>,
    pub hours:             serde_json::Value,
    pub status:            VendorStatus,
    pub created_at:        DateTime<Utc>,
    pub updated_at:        DateTime<Utc>,
}

impl Vendor {
    pub fn new(
        tenant_id: Uuid,
        vertical: Vertical,
        name: String,
        address: String,
        lat: f64,
        lng: f64,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            vertical,
            name,
            address,
            lat,
            lng,
            prep_time_minutes: 15,
            commission_bps: 1500,
            payout_account: None,
            hours: serde_json::json!({}),
            status: VendorStatus::Onboarding,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn is_orderable(&self) -> bool {
        self.status == VendorStatus::Active
    }

    pub fn activate(&mut self) { self.status = VendorStatus::Active; self.updated_at = Utc::now(); }
    pub fn pause(&mut self)    { self.status = VendorStatus::Paused; self.updated_at = Utc::now(); }

    /// The Partner's commission on a goods subtotal, in cents.
    ///
    /// Integer maths throughout, truncating. Truncation rounds in the vendor's
    /// favour, which is the correct direction for a fee the platform charges.
    pub fn commission_on(&self, subtotal_cents: i64) -> i64 {
        subtotal_cents * self.commission_bps as i64 / 10_000
    }

    /// What the vendor is credited for a goods subtotal, in cents.
    /// Always exactly `subtotal - commission`, so the two can never drift.
    pub fn payout_on(&self, subtotal_cents: i64) -> i64 {
        subtotal_cents - self.commission_on(subtotal_cents)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn vendor() -> Vendor {
        Vendor::new(Uuid::new_v4(), Vertical::Restaurant, "Kuya's Silog House".into(),
                    "123 Mabini St".into(), 14.5995, 120.9842)
    }

    #[test]
    fn a_new_vendor_starts_onboarding_and_is_not_orderable() {
        let v = vendor();
        assert_eq!(v.status, VendorStatus::Onboarding);
        assert!(!v.is_orderable());
    }

    #[test]
    fn only_an_active_vendor_is_orderable() {
        let mut v = vendor();
        v.activate();
        assert!(v.is_orderable());
        v.pause();
        assert!(!v.is_orderable());
    }

    /// Commission is the Partner's revenue leg. Basis-point maths must round
    /// down so the platform never over-charges a vendor by a rounding cent.
    #[test]
    fn commission_rounds_down_in_the_vendors_favour() {
        let mut v = vendor();
        v.commission_bps = 1500; // 15%

        assert_eq!(v.commission_on(10_000), 1_500);
        // 999 * 0.15 = 149.85 → 149, not 150
        assert_eq!(v.commission_on(999), 149);
    }

    #[test]
    fn payout_is_the_subtotal_less_commission() {
        let mut v = vendor();
        v.commission_bps = 1500;
        assert_eq!(v.payout_on(10_000), 8_500);
        assert_eq!(v.commission_on(10_000) + v.payout_on(10_000), 10_000,
                   "commission and payout must always sum to the subtotal");
    }

    #[test]
    fn zero_commission_pays_out_the_whole_subtotal() {
        let mut v = vendor();
        v.commission_bps = 0;
        assert_eq!(v.payout_on(12_345), 12_345);
    }
}
