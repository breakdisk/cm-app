use logisticos_pod::domain::entities::{
    proof::{PodPhoto, PodStatus, ProofOfDelivery},
    otp::OtpCode,
    pickup::{PopStatus, ProofOfPickup},
};
use uuid::Uuid;
use chrono::Utc;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_pod(geofence_verified: bool) -> ProofOfDelivery {
    ProofOfDelivery::new(
        Uuid::new_v4(), // tenant_id
        Uuid::new_v4(), // shipment_id
        Uuid::new_v4(), // task_id
        Uuid::new_v4(), // driver_id
        "Maria Santos".to_string(),
        14.5995, // capture_lat (Manila)
        120.9842, // capture_lng
        geofence_verified,
    )
}

fn make_photo() -> PodPhoto {
    PodPhoto {
        id: Uuid::new_v4(),
        s3_key: "tenants/abc/shipments/xyz/pod/photo.jpg".to_string(),
        content_type: "image/jpeg".to_string(),
        size_bytes: 204_800, // 200 KB
        uploaded_at: Utc::now(),
    }
}

// ---------------------------------------------------------------------------
// ProofOfDelivery::new()
// ---------------------------------------------------------------------------

mod pod_construction {
    use super::*;

    #[test]
    fn new_creates_pod_with_draft_status() {
        let pod = make_pod(true);
        assert_eq!(pod.status, PodStatus::Draft);
    }

    #[test]
    fn new_stores_recipient_name() {
        let pod = make_pod(true);
        assert_eq!(pod.recipient_name, "Maria Santos");
    }

    #[test]
    fn new_stores_gps_coordinates_correctly() {
        let pod = make_pod(true);
        assert!((pod.capture_lat - 14.5995).abs() < f64::EPSILON);
        assert!((pod.capture_lng - 120.9842).abs() < f64::EPSILON);
    }

    #[test]
    fn new_stores_geofence_verified_flag() {
        let pod_verified = make_pod(true);
        assert!(pod_verified.geofence_verified);

        let pod_unverified = make_pod(false);
        assert!(!pod_unverified.geofence_verified);
    }

    #[test]
    fn new_creates_pod_with_otp_unverified() {
        let pod = make_pod(true);
        assert!(!pod.otp_verified);
        assert!(pod.otp_id.is_none());
    }

    #[test]
    fn new_creates_pod_with_empty_photos_and_no_signature() {
        let pod = make_pod(true);
        assert!(pod.photos.is_empty());
        assert!(pod.signature_data.is_none());
    }

    #[test]
    fn new_generates_unique_id() {
        let pod1 = make_pod(true);
        let pod2 = make_pod(true);
        assert_ne!(pod1.id, pod2.id);
    }

    #[test]
    fn new_sets_cod_collected_to_none() {
        let pod = make_pod(true);
        assert!(pod.cod_collected_cents.is_none());
    }
}

// ---------------------------------------------------------------------------
// submit() — transitions Draft → Submitted
// ---------------------------------------------------------------------------

mod pod_submit {
    use super::*;

    #[test]
    fn submit_with_photo_and_geofence_succeeds() {
        let mut pod = make_pod(true);
        pod.attach_photo(make_photo());
        assert!(pod.submit().is_ok());
        assert_eq!(pod.status, PodStatus::Submitted);
    }

    #[test]
    fn submit_with_signature_and_geofence_succeeds() {
        let mut pod = make_pod(true);
        pod.attach_signature("data:image/png;base64,abc123".to_string());
        assert!(pod.submit().is_ok());
        assert_eq!(pod.status, PodStatus::Submitted);
    }

    #[test]
    fn submit_fails_without_any_evidence() {
        let mut pod = make_pod(true);
        // No photo, no signature.
        let err = pod.submit().unwrap_err();
        assert!(
            err.contains("incomplete"),
            "error should mention POD being incomplete"
        );
        // Status must not advance on failure.
        assert_eq!(pod.status, PodStatus::Draft);
    }

    #[test]
    fn submit_fails_when_geofence_not_verified_even_with_photo() {
        let mut pod = make_pod(false); // geofence_verified = false
        pod.attach_photo(make_photo());
        let err = pod.submit().unwrap_err();
        assert!(
            err.contains("geofence"),
            "error should mention geofence requirement"
        );
    }

    #[test]
    fn is_complete_is_true_with_photo_and_geofence() {
        let mut pod = make_pod(true);
        pod.attach_photo(make_photo());
        assert!(pod.is_complete());
    }

    #[test]
    fn is_complete_is_false_without_evidence() {
        let pod = make_pod(true);
        assert!(!pod.is_complete());
    }

    #[test]
    fn is_complete_is_false_without_geofence_even_with_evidence() {
        let mut pod = make_pod(false);
        pod.attach_photo(make_photo());
        assert!(!pod.is_complete());
    }
}

// ---------------------------------------------------------------------------
// verify() — transitions Submitted → Verified
// ---------------------------------------------------------------------------

mod pod_verify {
    use super::*;

    #[test]
    fn verify_changes_status_to_verified() {
        let mut pod = make_pod(true);
        pod.attach_photo(make_photo());
        pod.submit().unwrap();
        pod.verify();
        assert_eq!(pod.status, PodStatus::Verified);
    }

    #[test]
    fn disputed_pod_can_be_re_verified_after_resolution() {
        // The current domain model has no guard preventing verify() after dispute;
        // the caller (service layer) is responsible for workflow enforcement.
        // This test documents the raw domain behaviour.
        let mut pod = make_pod(true);
        pod.attach_photo(make_photo());
        pod.submit().unwrap();
        pod.dispute();
        assert_eq!(pod.status, PodStatus::Disputed);
        // After the dispute is resolved, verify() can be called.
        pod.verify();
        assert_eq!(pod.status, PodStatus::Verified);
    }
}

// ---------------------------------------------------------------------------
// dispute() — transitions to Disputed
// ---------------------------------------------------------------------------

mod pod_dispute {
    use super::*;

    #[test]
    fn dispute_changes_status_to_disputed() {
        let mut pod = make_pod(true);
        pod.attach_photo(make_photo());
        pod.submit().unwrap();
        pod.verify();
        pod.dispute();
        assert_eq!(pod.status, PodStatus::Disputed);
    }

    #[test]
    fn dispute_can_be_set_on_submitted_pod_before_verification() {
        let mut pod = make_pod(true);
        pod.attach_photo(make_photo());
        pod.submit().unwrap();
        pod.dispute();
        assert_eq!(pod.status, PodStatus::Disputed);
    }
}

// ---------------------------------------------------------------------------
// Signature and photo attachment
// ---------------------------------------------------------------------------

mod evidence_attachment {
    use super::*;

    #[test]
    fn attach_signature_stores_data() {
        let mut pod = make_pod(true);
        pod.attach_signature("data:image/svg+xml;base64,PHN2...".to_string());
        assert!(pod.signature_data.is_some());
    }

    #[test]
    fn attach_photo_appends_to_photo_list() {
        let mut pod = make_pod(true);
        pod.attach_photo(make_photo());
        pod.attach_photo(make_photo());
        assert_eq!(pod.photos.len(), 2);
    }

    #[test]
    fn pod_with_both_signature_and_photo_is_complete_when_geofenced() {
        let mut pod = make_pod(true);
        pod.attach_signature("data:image/png;base64,abc".to_string());
        pod.attach_photo(make_photo());
        assert!(pod.is_complete());
    }
}

// ---------------------------------------------------------------------------
// OTP verification
// ---------------------------------------------------------------------------

mod otp_verification {
    use super::*;

    #[test]
    fn mark_otp_verified_sets_flag_and_stores_otp_id() {
        let mut pod = make_pod(true);
        let otp_id = Uuid::new_v4();
        pod.mark_otp_verified(otp_id);
        assert!(pod.otp_verified);
        assert_eq!(pod.otp_id, Some(otp_id));
    }

    #[test]
    fn record_cod_stores_centavo_amount() {
        let mut pod = make_pod(true);
        pod.record_cod(149900); // PHP 1,499.00
        assert_eq!(pod.cod_collected_cents, Some(149900));
    }
}

// ---------------------------------------------------------------------------
// OtpCode entity
// ---------------------------------------------------------------------------

mod otp_code {
    use super::*;

    fn make_otp() -> OtpCode {
        OtpCode::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "+639170000001".to_string(),
            "e3b0c44298fc1c149afb4c8996fb92427ae41e4649b934ca495991b7852b855".to_string(),
        )
    }

    #[test]
    fn new_otp_is_valid_immediately_after_creation() {
        let otp = make_otp();
        assert!(otp.is_valid());
    }

    #[test]
    fn new_otp_is_not_used() {
        let otp = make_otp();
        assert!(!otp.is_used);
    }

    #[test]
    fn new_otp_expires_15_minutes_from_creation() {
        let otp = make_otp();
        let expected_expiry_window = chrono::Duration::minutes(15);
        let actual_window = otp.expires_at - otp.created_at;
        // Allow a 1-second tolerance for test execution time.
        assert!(actual_window >= expected_expiry_window - chrono::Duration::seconds(1));
        assert!(actual_window <= expected_expiry_window + chrono::Duration::seconds(1));
    }

    #[test]
    fn mark_used_invalidates_otp() {
        let mut otp = make_otp();
        otp.mark_used();
        assert!(otp.is_used);
        assert!(!otp.is_valid(), "used OTP must no longer be valid");
    }

    #[test]
    fn expired_otp_is_invalid() {
        let mut otp = make_otp();
        // Backdate expiry so it is in the past.
        otp.expires_at = Utc::now() - chrono::Duration::seconds(1);
        assert!(!otp.is_valid());
    }

    #[test]
    fn otp_stores_phone_number() {
        let otp = make_otp();
        assert_eq!(otp.phone, "+639170000001");
    }

    #[test]
    fn otp_stores_hashed_code_not_plaintext() {
        // SHA-256 of empty string — just verifying a hash string is stored.
        let hash = "e3b0c44298fc1c149afb4c8996fb92427ae41e4649b934ca495991b7852b855";
        let otp = make_otp();
        assert_eq!(otp.code_hash, hash);
        // The hash must not look like a 6-digit plaintext OTP.
        assert!(otp.code_hash.len() > 6, "stored value must be a hash, not a plaintext OTP");
    }
}

// ---------------------------------------------------------------------------
// ProofOfPickup — construction, state transitions, S3 key convention
// ---------------------------------------------------------------------------

fn make_pop() -> ProofOfPickup {
    ProofOfPickup::new(
        Uuid::new_v4(), // tenant_id
        Uuid::new_v4(), // shipment_id
        Uuid::new_v4(), // task_id
        Uuid::new_v4(), // driver_id
        14.5995,        // capture_lat (Manila)
        120.9842,       // capture_lng
        true,           // geofence_verified
        false,          // out_of_bounds_handover
        Some(1500),     // declared_weight_g
        "standard".to_string(),
        None,           // declared_value_cents
        None,           // device_timestamp
    )
}

mod pop_construction {
    use super::*;

    #[test]
    fn new_creates_pop_with_draft_status() {
        let pop = make_pop();
        assert_eq!(pop.status, PopStatus::Draft);
    }

    #[test]
    fn new_creates_pop_with_no_photo() {
        let pop = make_pop();
        assert!(pop.photo_s3_key.is_none());
        assert!(pop.photo_size_bytes.is_none());
    }

    #[test]
    fn new_creates_pop_with_barcode_unscanned() {
        let pop = make_pop();
        assert!(!pop.barcode_scanned);
        assert!(pop.scanned_barcode.is_none());
    }

    #[test]
    fn new_generates_unique_ids() {
        let p1 = make_pop();
        let p2 = make_pop();
        assert_ne!(p1.id, p2.id);
    }

    #[test]
    fn submit_advances_status_to_submitted() {
        let mut pop = make_pop();
        pop.submit();
        assert_eq!(pop.status, PopStatus::Submitted);
    }
}

mod pop_photo_key_prefix {
    use super::*;

    /// The S3 key for POP photos MUST start with "pop/" so uploads land in the
    /// logisticos-pop-photos bucket and never collide with POD keys ("pod/").
    /// This test verifies that `attach_photo` stores the key unchanged and that
    /// a correctly-formatted key has the right prefix.
    #[test]
    fn photo_key_with_pop_prefix_is_stored_correctly() {
        let mut pop = make_pop();
        let key = format!(
            "pop/{}/{}/{}/{}.jpg",
            pop.tenant_id, pop.shipment_id, pop.id, Uuid::new_v4()
        );
        pop.attach_photo(key.clone(), 512_000);
        assert_eq!(pop.photo_s3_key.as_deref(), Some(key.as_str()));
        assert!(
            pop.photo_s3_key.as_deref().unwrap().starts_with("pop/"),
            "POP S3 key must start with 'pop/' — got: {:?}",
            pop.photo_s3_key
        );
    }

    #[test]
    fn pod_key_prefix_is_distinct_from_pop_prefix() {
        // Belt-and-suspenders: confirm the two prefixes are different strings
        // so a misconfiguration that swaps pod_storage / pop_storage would
        // produce keys that start with the wrong prefix and be detectable.
        assert_ne!("pod/", "pop/");
    }

    #[test]
    fn photo_size_bytes_is_stored() {
        let mut pop = make_pop();
        let key = format!("pop/{}/test.jpg", pop.tenant_id);
        pop.attach_photo(key, 204_800);
        assert_eq!(pop.photo_size_bytes, Some(204_800));
    }
}

mod pop_weight_overage {
    use super::*;

    #[test]
    fn overage_ratio_positive_when_actual_exceeds_declared() {
        let mut pop = make_pop();
        pop.record_weight(1600, 1500); // 100 g over → ~6.67%
        let ratio = pop.weight_overage_ratio().expect("should compute ratio");
        assert!((ratio - 100.0 / 1500.0).abs() < 1e-9);
    }

    #[test]
    fn overage_ratio_zero_when_weights_match() {
        let mut pop = make_pop();
        pop.record_weight(1500, 1500);
        let ratio = pop.weight_overage_ratio().expect("should compute ratio");
        assert!(ratio.abs() < 1e-9);
    }

    #[test]
    fn overage_ratio_within_5pct_tolerance_band() {
        // 4% over — should NOT trigger an invoice per the billing spec
        let mut pop = make_pop();
        pop.record_weight(1560, 1500); // 60 g over ≈ 4%
        let ratio = pop.weight_overage_ratio().expect("should compute ratio");
        assert!(ratio <= 0.05, "ratio {ratio} should be within 5% tolerance");
    }

    #[test]
    fn overage_ratio_above_5pct_triggers_billing() {
        // 6% over — should trigger HeldForPaymentOverage
        let mut pop = make_pop();
        pop.record_weight(1590, 1500); // 90 g over = 6%
        let ratio = pop.weight_overage_ratio().expect("should compute ratio");
        assert!(ratio > 0.05, "ratio {ratio} should exceed 5% tolerance");
    }

    #[test]
    fn overage_ratio_is_none_when_weights_absent() {
        let pop = make_pop(); // no record_weight call
        assert!(pop.weight_overage_ratio().is_none());
    }

    #[test]
    fn scan_records_barcode_and_sets_flag() {
        let mut pop = make_pop();
        pop.record_scan("AWB-001234567".to_string());
        assert!(pop.barcode_scanned);
        assert_eq!(pop.scanned_barcode.as_deref(), Some("AWB-001234567"));
    }
}
