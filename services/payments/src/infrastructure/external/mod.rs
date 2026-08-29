pub mod identity_client;
// Payment gateway adapters: PayMongo, GCash, Maya (future).
// PayMongo integration for card payments and GCash e-wallet withdrawals.

pub mod network_international;
pub use network_international::NetworkInternationalGateway;
