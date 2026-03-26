// Redis cache for driver-ops.
// Stores active driver sessions and WebSocket connection metadata.
// Location updates are pushed to Kafka; Redis holds the latest position for sub-millisecond reads.
