use logisticos_pod::domain::entities::{
    proof::{PodPhoto, PodStatus, ProofOfDelivery},
    otp::OtpCode,
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
