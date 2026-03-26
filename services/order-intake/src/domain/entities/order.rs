// An "Order" in this service is the same concept as a Shipment at intake time.
// This module re-exports Shipment so downstream code can reference either name.
pub use super::shipment::Shipment as Order;
