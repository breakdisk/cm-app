use std::sync::Arc;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use logisticos_errors::{AppError, AppResult};
use logisticos_types::TenantId;

use crate::domain::{
    entities::{Vehicle, VehicleId, VehicleType},
    repositories::VehicleRepository,
};

#[derive(Debug, Deserialize)]
pub struct CreateVehicleCommand {
    pub plate_number:  String,
    pub vehicle_type:  VehicleType,
    pub make:          String,
    pub model:         String,
    pub year:          u16,
    pub color:         String,
}

#[derive(Debug, Deserialize)]
pub struct ScheduleMaintenanceCommand {
    pub description:    String,
    pub scheduled_date: NaiveDate,
}

#[derive(Debug, Deserialize)]
pub struct CompleteMaintenanceCommand {
    pub odometer_km: i32,
    pub cost_cents:  i64,
    pub notes:       Option<String>,
}

pub struct FleetService {
    repo: Arc<dyn VehicleRepository>,
}

impl FleetService {
    pub fn new(repo: Arc<dyn VehicleRepository>) -> Self { Self { repo } }

    pub async fn create(&self, tenant_id: &TenantId, cmd: CreateVehicleCommand) -> AppResult<Vehicle> {
        let vehicle = Vehicle::new(
            tenant_id.clone(),
            cmd.plate_number,
            cmd.vehicle_type,
            cmd.make,
            cmd.model,
            cmd.year,
            cmd.color,
        );
        self.repo.save(&vehicle).await.map_err(AppError::internal)?;
        Ok(vehicle)
    }

    pub async fn get(&self, id: Uuid) -> AppResult<Vehicle> {
        self.repo
            .find_by_id(&VehicleId::from_uuid(id))
            .await
            .map_err(AppError::internal)?
            .ok_or_else(|| AppError::NotFound { resource: "Vehicle", id: id.to_string() })
    }

    pub async fn list(&self, tenant_id: &TenantId, limit: i64, offset: i64) -> AppResult<Vec<Vehicle>> {
        self.repo
            .list(tenant_id, limit.clamp(1, 200), offset.max(0))
            .await
            .map_err(AppError::internal)
    }

    pub async fn assign_driver(&self, vehicle_id: Uuid, driver_id: Uuid) -> AppResult<Vehicle> {
        let mut vehicle = self.get(vehicle_id).await?;
        vehicle.assign_driver(driver_id).map_err(|e| AppError::BusinessRule(e.to_string()))?;
        self.repo.save(&vehicle).await.map_err(AppError::internal)?;
        Ok(vehicle)
    }

    pub async fn unassign_driver(&self, vehicle_id: Uuid) -> AppResult<Vehicle> {
        let mut vehicle = self.get(vehicle_id).await?;
        vehicle.unassign_driver();
        self.repo.save(&vehicle).await.map_err(AppError::internal)?;
        Ok(vehicle)
    }

    pub async fn schedule_maintenance(
        &self,
        vehicle_id: Uuid,
        cmd: ScheduleMaintenanceCommand,
    ) -> AppResult<Vehicle> {
        let mut vehicle = self.get(vehicle_id).await?;
        vehicle.schedule_maintenance(cmd.description, cmd.scheduled_date);
        self.repo.save(&vehicle).await.map_err(AppError::internal)?;
        Ok(vehicle)
    }

    pub async fn complete_maintenance(
        &self,
        vehicle_id: Uuid,
        cmd: CompleteMaintenanceCommand,
    ) -> AppResult<Vehicle> {
        let mut vehicle = self.get(vehicle_id).await?;
        vehicle
            .complete_maintenance(cmd.odometer_km, cmd.cost_cents, cmd.notes)
            .map_err(|e| AppError::BusinessRule(e.to_string()))?;
        self.repo.save(&vehicle).await.map_err(AppError::internal)?;
        Ok(vehicle)
    }

    pub async fn decommission(&self, vehicle_id: Uuid) -> AppResult<Vehicle> {
        let mut vehicle = self.get(vehicle_id).await?;
        vehicle.decommission();
        self.repo.save(&vehicle).await.map_err(AppError::internal)?;
        Ok(vehicle)
    }

    pub async fn maintenance_due_alerts(
        &self,
        tenant_id: &TenantId,
        within_days: i64,
    ) -> AppResult<Vec<Vehicle>> {
        self.repo
            .list_maintenance_due(tenant_id, within_days.clamp(1, 90))
            .await
            .map_err(AppError::internal)
    }
}
