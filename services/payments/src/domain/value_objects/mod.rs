/// Philippine VAT rate — 12% as of 2024.
pub const PH_VAT_RATE: f64 = 0.12;

/// Platform COD handling fee — 1.5% of collected amount.
pub const COD_HANDLING_FEE_RATE: f64 = 0.015;

/// Payment terms for merchant invoices (days).
pub const NET_PAYMENT_TERMS_DAYS: i64 = 15;

/// Minimum withdrawal amount from merchant wallet (₱500 = 50000 centavos).
pub const MIN_WITHDRAWAL_CENTS: i64 = 50_000;

/// Calculate VAT amount from a subtotal (in centavos).
pub fn compute_vat(subtotal_cents: i64) -> i64 {
    (subtotal_cents as f64 * PH_VAT_RATE).round() as i64
}

/// Calculate the platform COD fee (in centavos).
pub fn compute_cod_fee(amount_cents: i64) -> i64 {
    (amount_cents as f64 * COD_HANDLING_FEE_RATE).round() as i64
}
