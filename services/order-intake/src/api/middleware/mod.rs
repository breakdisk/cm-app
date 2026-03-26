// Axum middleware specific to order-intake:
//   - Idempotency key validation (X-Idempotency-Key header)
//   - Merchant rate-limit enforcement
// These are applied per-route in bootstrap.rs via tower layers.
