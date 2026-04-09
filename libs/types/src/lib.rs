pub mod awb;
pub mod invoice;

pub use awb::{Awb, ChildAwb, AwbError, ServiceCode, TenantCode};
pub use invoice::{InvoiceNumber, InvoiceNumberError, RemittanceNumber, CreditNoteNumber};

use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

// ── Macro: typed UUID newtype ────────────────────────────────
macro_rules! typed_id {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub struct $name(pub Uuid);

        impl $name {
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }
            pub fn from_uuid(id: Uuid) -> Self {
                Self(id)
            }
            pub fn inner(&self) -> Uuid {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}

// ── Identity types ───────────────────────────────────────────
typed_id!(TenantId);
typed_id!(OrganizationId);
typed_id!(BranchId);
typed_id!(UserId);
typed_id!(ApiKeyId);
typed_id!(RoleId);

// ── Logistics types ──────────────────────────────────────────
typed_id!(ShipmentId);
typed_id!(OrderId);
typed_id!(RouteId);
typed_id!(WaybillId);
typed_id!(PickupId);
typed_id!(DeliveryId);
typed_id!(DriverId);
typed_id!(VehicleId);
typed_id!(HubId);
typed_id!(ZoneId);
typed_id!(CarrierId);
typed_id!(ProofOfDeliveryId);
typed_id!(PalletId);
typed_id!(ContainerId);
typed_id!(LineItemId);

// ── Customer / Merchant types ────────────────────────────────
typed_id!(CustomerId);
typed_id!(MerchantId);
typed_id!(CampaignId);
typed_id!(SegmentId);

// ── Financial types ──────────────────────────────────────────
typed_id!(InvoiceId);
typed_id!(PaymentId);
typed_id!(WalletId);
typed_id!(TransactionId);

// ── Money ────────────────────────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Money {
    /// Amount in the smallest currency unit (e.g., centavos for PHP, cents for USD)
    pub amount: i64,
    pub currency: Currency,
}

impl Money {
    pub fn new(amount: i64, currency: Currency) -> Self {
        Self { amount, currency }
    }
    pub fn zero(currency: Currency) -> Self {
        Self { amount: 0, currency }
    }
    pub fn add(self, other: Money) -> Result<Money, &'static str> {
        if self.currency != other.currency {
            return Err("Currency mismatch");
        }
        Ok(Money { amount: self.amount + other.amount, currency: self.currency })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Currency {
    PHP,
    USD,
    SGD,
    MYR,
    IDR,
}

// ── Address ──────────────────────────────────────────────────
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Address {
    pub line1: String,
    pub line2: Option<String>,
    pub barangay: Option<String>,
    pub city: String,
    pub province: String,
    pub postal_code: String,
    pub country_code: String,
    pub coordinates: Option<Coordinates>,
}

// ── Geospatial ───────────────────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Coordinates {
    pub lat: f64,
    pub lng: f64,
}

impl Coordinates {
    /// Haversine distance in kilometers
    pub fn distance_km(&self, other: &Coordinates) -> f64 {
        const R: f64 = 6371.0;
        let dlat = (other.lat - self.lat).to_radians();
        let dlng = (other.lng - self.lng).to_radians();
        let a = (dlat / 2.0).sin().powi(2)
            + self.lat.to_radians().cos()
            * other.lat.to_radians().cos()
            * (dlng / 2.0).sin().powi(2);
        let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
        R * c
    }
}

// ── Subscription Tiers ───────────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubscriptionTier {
    Starter,
    Growth,
    Business,
    Enterprise,
}

impl SubscriptionTier {
    pub fn max_monthly_shipments(&self) -> Option<u64> {
        match self {
            SubscriptionTier::Starter    => Some(500),
            SubscriptionTier::Growth     => Some(5_000),
            SubscriptionTier::Business   => Some(50_000),
            SubscriptionTier::Enterprise => None, // unlimited
        }
    }

    pub fn allows_ai_features(&self) -> bool {
        matches!(self, SubscriptionTier::Business | SubscriptionTier::Enterprise)
    }

    pub fn allows_white_label(&self) -> bool {
        matches!(self, SubscriptionTier::Enterprise)
    }
}

// ── Shipment Status ──────────────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShipmentStatus {
    Pending,
    Confirmed,
    PickupAssigned,
    PickedUp,
    InTransit,
    AtHub,
    OutForDelivery,
    DeliveryAttempted,
    Delivered,
    /// Some pieces delivered, not all (multi-piece shipments only).
    PartialDelivery,
    /// One or more pieces flagged as damaged or missing.
    PieceException,
    /// Shipment held at customs (international / Balikbayan).
    CustomsHold,
    Failed,
    Cancelled,
    Returned,
}

// ── Piece Status ─────────────────────────────────────────────
/// Status of an individual piece within a multi-piece shipment.
/// Tracked independently at hub scan level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PieceStatus {
    /// Created at booking, not yet received at hub.
    Pending,
    /// Scanned inbound at origin hub.
    ScannedIn,
    /// Loaded onto pallet or container for linehaul.
    InTransit,
    /// Scanned outbound from hub / loaded on last-mile vehicle.
    ScannedOut,
    /// Delivered to consignee, POD captured.
    Delivered,
    /// Expected but not scanned — under investigation.
    Missing,
    /// Physically damaged — exception raised.
    Damaged,
}

// ── Pallet Status ────────────────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PalletStatus {
    /// Accepting pieces, not yet sealed.
    Open,
    /// Wrapped, weighed, labelled — no more pieces can be added.
    Sealed,
    /// Loaded into a container or vehicle.
    Loaded,
    InTransit,
    /// Scanned at destination hub.
    Arrived,
    /// Pallet broken up at destination hub; pieces distributed for last-mile.
    Broken,
}

// ── Container Status ─────────────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContainerStatus {
    /// Manifest being built.
    Planning,
    /// Manifest finalised, awaiting physical load.
    Manifested,
    /// Pieces/pallets being loaded.
    Loading,
    /// Sealed and ready to depart.
    Sealed,
    InTransit,
    /// Arrived at port/airport, awaiting customs clearance.
    ArrivedAtPort,
    /// Held at customs.
    Customs,
    /// Customs cleared, released for onward movement.
    Released,
    /// Arrived at destination hub, fully unloaded.
    Delivered,
}

// ── Transport Mode ───────────────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransportMode {
    /// Hub-to-hub road truck.
    Road,
    /// Sea freight — full container load.
    SeaFcl,
    /// Sea freight — less than container load (consolidated).
    SeaLcl,
    /// Air — unit load device (airline pallet/container).
    AirUld,
    /// Air — loose freight not in a ULD.
    AirLoose,
}

// ── Pagination ───────────────────────────────────────────────
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pagination {
    pub page: u64,
    pub per_page: u64,
}

impl Default for Pagination {
    fn default() -> Self {
        Self { page: 1, per_page: 20 }
    }
}

impl Pagination {
    pub fn offset(&self) -> i64 {
        ((self.page.saturating_sub(1)) * self.per_page) as i64
    }
    pub fn limit(&self) -> i64 {
        self.per_page as i64
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedResponse<T> {
    pub data: Vec<T>,
    pub total: u64,
    pub page: u64,
    pub per_page: u64,
    pub total_pages: u64,
}

impl<T> PaginatedResponse<T> {
    pub fn new(data: Vec<T>, total: u64, pagination: &Pagination) -> Self {
        let total_pages = (total + pagination.per_page - 1) / pagination.per_page;
        Self { data, total, page: pagination.page, per_page: pagination.per_page, total_pages }
    }
}
