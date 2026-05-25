/// DIM factor used throughout the platform (cm³ per kg).
/// Standard value: 5 000 — same constant as order-intake's volumetric weight formula.
pub const DIM_FACTOR_CM3_PER_KG: f64 = 5_000.0;

/// Estimate a cubic bounding box from weight alone (used when L/W/H are unknown).
///
/// Formula: volume_cm3 = weight_g / 1000 * DIM_FACTOR; side = cbrt(volume_cm3).
/// Returns (length_cm, width_cm, height_cm) — a cube with the estimated side length.
/// Minimum side is 5 cm (avoids degenerate zero-size boxes).
pub fn estimate_dims_cm(weight_grams: u32) -> (u32, u32, u32) {
    if weight_grams == 0 {
        return (5, 5, 5);
    }
    let volume_cm3 = weight_grams as f64 / 1000.0 * DIM_FACTOR_CM3_PER_KG;
    let side = (volume_cm3.cbrt().round() as u32).max(5);
    (side, side, side)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_weight_returns_floor() {
        let (l, w, h) = estimate_dims_cm(0);
        assert_eq!((l, w, h), (5, 5, 5));
    }

    #[test]
    fn standard_jumbo_box_weight() {
        // Jumbo Balikbayan: ~30 kg → DIM vol = 30*5000 = 150_000 cm³ → cbrt ≈ 53 cm
        let (l, w, h) = estimate_dims_cm(30_000);
        assert_eq!(l, w);
        assert_eq!(w, h);
        assert!(l >= 50 && l <= 56, "expected ~53 cm, got {l}");
    }
}
