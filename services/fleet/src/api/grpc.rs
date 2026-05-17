/// gRPC services for inter-service communication
/// Future: define protobuf services for fleet.Vehicle queries used by marketplace,
/// dispatch, and other services to check vehicle operational status.

pub mod vehicle_service {
    /// GetVehicleStatus — used by marketplace DMS to validate vehicle is operational
    /// Request: vehicle_id (UUID)
    /// Response: status (active|maintenance|decommissioned), last_updated_at (timestamp)
    pub const SERVICE_NAME: &str = "fleet.FleetService";
}
