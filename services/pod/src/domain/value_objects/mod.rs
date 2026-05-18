/// Maximum photo size allowed per delivery (5 MB).
pub const MAX_PHOTO_SIZE_BYTES: u64 = 5 * 1024 * 1024;

/// Maximum number of photos per POD.
pub const MAX_PHOTOS_PER_POD: usize = 5;

/// Allowed photo content types.
pub const ALLOWED_PHOTO_TYPES: &[&str] = &["image/jpeg", "image/png", "image/webp"];

/// OTP code length — 6 digits.
pub const OTP_LENGTH: usize = 6;

/// Geofence for POD capture — driver must be within this radius of the delivery address.
/// Controls `geofence_verified` on the entity (hard check at initiate time).
pub const POD_GEOFENCE_METERS: f64 = 200.0;

/// Soft audit threshold for out-of-bounds handover annotation.
/// When the driver is farther than this value from the address polygon edge,
/// `out_of_bounds_handover = true` is set on the POD/POP entity and written to
/// billing `workflow_metadata`. NOT a block — only an audit flag for SLA review.
/// Applies to both pickup and delivery; distinct from the 200 m geofence.
pub const OUT_OF_BOUNDS_HANDOVER_METERS: f64 = 50.0;

pub fn is_allowed_content_type(content_type: &str) -> bool {
    ALLOWED_PHOTO_TYPES.contains(&content_type)
}

/// Generate a cryptographically secure random 6-digit OTP code.
pub fn generate_otp() -> String {
    use rand::{Rng, rngs::OsRng};
    format!("{:06}", OsRng.gen_range(0u32..1_000_000))
}

/// SHA-256 hash of an OTP code for safe storage.
pub fn hash_otp(code: &str) -> String {
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(code.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Verify an OTP code against its stored hash using constant-time comparison.
pub fn verify_otp(code: &str, stored_hash: &str) -> bool {
    use subtle::ConstantTimeEq;
    let a = hash_otp(code);
    a.as_bytes().ct_eq(stored_hash.as_bytes()).into()
}
