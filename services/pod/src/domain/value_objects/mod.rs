/// Maximum photo size allowed per delivery (5 MB).
pub const MAX_PHOTO_SIZE_BYTES: u64 = 5 * 1024 * 1024;

/// Maximum number of photos per POD.
pub const MAX_PHOTOS_PER_POD: usize = 5;

/// Allowed photo content types.
pub const ALLOWED_PHOTO_TYPES: &[&str] = &["image/jpeg", "image/png", "image/webp"];

/// OTP code length — 6 digits.
pub const OTP_LENGTH: usize = 6;

/// Geofence for POD capture — must be within 200m of delivery address.
pub const POD_GEOFENCE_METERS: f64 = 200.0;

pub fn is_allowed_content_type(content_type: &str) -> bool {
    ALLOWED_PHOTO_TYPES.contains(&content_type)
}

/// Generate a random 6-digit OTP code.
pub fn generate_otp() -> String {
    use std::time::SystemTime;
    let seed = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    // XOR-shift for deterministic randomness — replace with rand::OsRng in production
    let mut state = seed as u64 ^ (seed as u64 >> 17) ^ 0xDEADBEEF;
    state ^= state << 13;
    state ^= state >> 7;
    state ^= state << 17;
    format!("{:06}", state % 1_000_000)
}

/// SHA-256 hash of an OTP code for safe storage.
pub fn hash_otp(code: &str) -> String {
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(code.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Verify an OTP code against its stored hash.
pub fn verify_otp(code: &str, stored_hash: &str) -> bool {
    hash_otp(code) == stored_hash
}
