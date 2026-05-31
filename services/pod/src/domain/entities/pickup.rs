use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Proof of Pickup (POP) — the chain-of-custody-open event recorded when a driver
/// collects a parcel from the merchant or hub. The POP is the bookend to POD:
/// POP opens the chain of custody; POD closes it.
///
/// Like POD, POP is evidence-grade: it stores GPS coordinates, an optional photo
/// of the parcel at collection, and a barcode scan confirming the physical AWB
/// matches the task. For Track B (Standard Parcel) it also carries the actual
/// weight measured at pickup, enabling overage surcharge computation at billing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofOfPickup {
    pub id:           Uuid,
    pub tenant_id:    Uuid,
    pub shipment_id:  Uuid,
    pub task_id:      Uuid,
    pub driver_id:    Uuid,
    pub status:       PopStatus,

    // GPS capture at moment of pickup
    pub capture_lat:  f64,
    pub capture_lng:  f64,

    /// True when the driver was within the 200m pickup geofence.
    /// Same semantics as `ProofOfDelivery::geofence_verified`.
    pub geofence_verified: bool,

    /// Soft audit flag — set when driver is > 50m from the pickup address polygon edge.
    /// NOT a block on submission; written to `workflow_metadata` on the billing invoice
    /// for audit and SLA review. Distinct from the 200m hard geofence: a reading between
    /// 51m and 200m sets this flag AND leaves `geofence_verified = true`.
    pub out_of_bounds_handover: bool,

    // Photo evidence (parcel condition at pickup — optional but recommended)
    pub photo_s3_key:     Option<String>,
    pub photo_size_bytes: Option<u64>,

    // Weight/dimension capture — required for Track B (Standard Parcel) to detect
    // overages against the declared weight at booking time.
    /// Actual weight in grams as measured at pickup.
    pub actual_weight_g:   Option<i64>,
    /// Declared weight in grams from the shipment booking.
    pub declared_weight_g: Option<i64>,

    // Barcode scan — confirms the physical AWB matches the task's shipment.
    pub barcode_scanned:  bool,
    pub scanned_barcode:  Option<String>,

    /// Service classification for billing routing — carried from the booking.
    /// "balikbayan" → Track A driver ledger debit; "standard" → Track B weight billing.
    pub service_code:         String,
    /// Declared value in cents — populated for Track A (Balikbayan) shipments.
    pub declared_value_cents: Option<i64>,

    // AR dimensioning captured at pickup (driver app VERIFY mode). All optional —
    // NULL for manual / non-AR pickups. Serves the POP size/quantity/fraud audit.
    pub verified_length_cm:   Option<f64>,
    pub verified_width_cm:    Option<f64>,
    pub verified_height_cm:   Option<f64>,
    pub verified_cbm:         Option<f64>,
    pub volumetric_weight_kg: Option<f64>,
    /// Number of identical boxes in this pickup.
    pub box_quantity:         Option<i32>,
    /// Anti-fraud score of the AR measurement: VERIFIED | REVIEW | FLAGGED | PENDING.
    pub dimension_integrity:  Option<String>,

    /// Hardware clock timestamp from the driver's device at the moment of scan/capture.
    /// Always use this for chain-of-custody audit and SLA calculations.
    /// Pair with `captured_at` (server receipt time) to detect clock drift > 5 minutes.
    pub device_timestamp: Option<DateTime<Utc>>,

    /// Server-assigned receipt timestamp (backend clock when the row was written).
    /// See `device_timestamp` for the canonical event time.
    pub captured_at: DateTime<Utc>,
    pub created_at:  DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PopStatus {
    /// In progress — evidence collection incomplete.
    Draft,
    /// Fully captured and submitted; immutable after this point.
    Submitted,
}

impl ProofOfPickup {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        tenant_id:              Uuid,
        shipment_id:            Uuid,
        task_id:                Uuid,
        driver_id:              Uuid,
        capture_lat:            f64,
        capture_lng:            f64,
        geofence_verified:      bool,
        out_of_bounds_handover: bool,
        declared_weight_g:      Option<i64>,
        service_code:           String,
        declared_value_cents:   Option<i64>,
        device_timestamp:       Option<DateTime<Utc>>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            shipment_id,
            task_id,
            driver_id,
            status: PopStatus::Draft,
            capture_lat,
            capture_lng,
            geofence_verified,
            out_of_bounds_handover,
            photo_s3_key: None,
            photo_size_bytes: None,
            actual_weight_g: None,
            declared_weight_g,
            barcode_scanned: false,
            scanned_barcode: None,
            service_code,
            declared_value_cents,
            verified_length_cm:   None,
            verified_width_cm:    None,
            verified_height_cm:   None,
            verified_cbm:         None,
            volumetric_weight_kg: None,
            box_quantity:         None,
            dimension_integrity:  None,
            device_timestamp,
            captured_at: now,
            created_at: now,
        }
    }

    /// Records AR-measured box dimensioning captured at pickup (VERIFY mode).
    #[allow(clippy::too_many_arguments)]
    pub fn record_dimensions(
        &mut self,
        length_cm:   Option<f64>,
        width_cm:    Option<f64>,
        height_cm:   Option<f64>,
        cbm:         Option<f64>,
        vol_kg:      Option<f64>,
        quantity:    Option<i32>,
        integrity:   Option<String>,
    ) {
        self.verified_length_cm   = length_cm;
        self.verified_width_cm    = width_cm;
        self.verified_height_cm   = height_cm;
        self.verified_cbm         = cbm;
        self.volumetric_weight_kg = vol_kg;
        self.box_quantity         = quantity;
        self.dimension_integrity  = integrity;
    }

    pub fn attach_photo(&mut self, s3_key: String, size_bytes: u64) {
        self.photo_s3_key = Some(s3_key);
        self.photo_size_bytes = Some(size_bytes);
    }

    pub fn record_scan(&mut self, barcode: String) {
        self.barcode_scanned = true;
        self.scanned_barcode = Some(barcode);
    }

    pub fn record_weight(&mut self, actual_g: i64, declared_g: i64) {
        self.actual_weight_g  = Some(actual_g);
        self.declared_weight_g = Some(declared_g);
    }

    /// Returns the weight overage ratio — positive when actual > declared.
    /// Callers apply the 5% tolerance band before deciding whether to invoice.
    /// Returns `None` when either weight is absent.
    pub fn weight_overage_ratio(&self) -> Option<f64> {
        let actual  = self.actual_weight_g? as f64;
        let declared = self.declared_weight_g?;
        if declared <= 0 { return None; }
        Some((actual - declared as f64) / declared as f64)
    }

    pub fn submit(&mut self) {
        self.status = PopStatus::Submitted;
    }
}
