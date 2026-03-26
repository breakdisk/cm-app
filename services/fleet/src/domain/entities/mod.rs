use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use logisticos_types::TenantId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VehicleId(Uuid);

impl VehicleId {
    pub fn new() -> Self { Self(Uuid::new_v4()) }
    pub fn from_uuid(id: Uuid) -> Self { Self(id) }
    pub fn inner(&self) -> Uuid { self.0 }
}

impl Default for VehicleId {
    fn default() -> Self { Self::new() }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VehicleType {
    Motorcycle,
    Van,
    Truck,
    Bicycle,
    Car,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VehicleStatus {
    Active,
    UnderMaintenance,
    Decommissioned,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaintenanceRecord {
    pub id:              Uuid,
    pub description:     String,
    pub scheduled_date:  NaiveDate,
    pub completed_at:    Option<DateTime<Utc>>,
    pub odometer_km:     Option<i32>,
    pub cost_cents:      Option<i64>,
    pub notes:           Option<String>,
}

impl MaintenanceRecord {
    pub fn new(description: String, scheduled_date: NaiveDate) -> Self {
        Self {
            id:             Uuid::new_v4(),
            description,
            scheduled_date,
            completed_at:  None,
            odometer_km:   None,
            cost_cents:    None,
            notes:         None,
        }
    }

    pub fn complete(&mut self, odometer_km: i32, cost_cents: i64, notes: Option<String>) {
        self.completed_at = Some(Utc::now());
        self.odometer_km  = Some(odometer_km);
        self.cost_cents   = Some(cost_cents);
        self.notes        = notes;
    }

    pub fn is_overdue(&self) -> bool {
        self.completed_at.is_none() && self.scheduled_date < chrono::Local::now().date_naive()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vehicle {
    pub id:               VehicleId,
    pub tenant_id:        TenantId,

    pub plate_number:     String,      // e.g. "ABC 1234"
    pub vehicle_type:     VehicleType,
    pub make:             String,      // e.g. "Honda"
    pub model:            String,      // e.g. "Beat"
    pub year:             u16,
    pub color:            String,

    pub status:           VehicleStatus,

    // Currently assigned driver (None if unassigned)
    pub assigned_driver_id: Option<Uuid>,

    // Odometer (km)
    pub odometer_km:      i32,

    // Maintenance log (JSONB in DB, last 20 records)
    pub maintenance_history: Vec<MaintenanceRecord>,

    // Next scheduled maintenance date
    pub next_maintenance_due: Option<NaiveDate>,

    pub created_at:       DateTime<Utc>,
    pub updated_at:       DateTime<Utc>,
}

impl Vehicle {
    pub fn new(
        tenant_id: TenantId,
        plate_number: String,
        vehicle_type: VehicleType,
        make: String,
        model: String,
        year: u16,
        color: String,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: VehicleId::new(),
            tenant_id,
            plate_number,
            vehicle_type,
            make,
            model,
            year,
            color,
            status: VehicleStatus::Active,
            assigned_driver_id: None,
            odometer_km: 0,
            maintenance_history: Vec::new(),
            next_maintenance_due: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Assign a driver to this vehicle.
    pub fn assign_driver(&mut self, driver_id: Uuid) -> anyhow::Result<()> {
        if self.status != VehicleStatus::Active {
            anyhow::bail!("Cannot assign driver to vehicle with status {:?}", self.status);
        }
        self.assigned_driver_id = Some(driver_id);
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Unassign the current driver.
    pub fn unassign_driver(&mut self) {
        self.assigned_driver_id = None;
        self.updated_at = Utc::now();
    }

    /// Schedule a maintenance event.
    pub fn schedule_maintenance(&mut self, description: String, date: NaiveDate) {
        let record = MaintenanceRecord::new(description, date);
        self.next_maintenance_due = Some(date);
        self.maintenance_history.push(record);
        // Keep only last 20 records.
        if self.maintenance_history.len() > 20 {
            self.maintenance_history.remove(0);
        }
        self.status = VehicleStatus::UnderMaintenance;
        self.updated_at = Utc::now();
    }

    /// Complete the latest scheduled maintenance.
    pub fn complete_maintenance(
        &mut self,
        odometer_km: i32,
        cost_cents: i64,
        notes: Option<String>,
    ) -> anyhow::Result<()> {
        let record = self
            .maintenance_history
            .iter_mut()
            .rev()
            .find(|r| r.completed_at.is_none())
            .ok_or_else(|| anyhow::anyhow!("No pending maintenance found"))?;
        record.complete(odometer_km, cost_cents, notes);
        self.odometer_km = odometer_km;
        self.status = VehicleStatus::Active;
        self.next_maintenance_due = None;
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn decommission(&mut self) {
        self.status = VehicleStatus::Decommissioned;
        self.assigned_driver_id = None;
        self.updated_at = Utc::now();
    }

    /// Vehicles due for maintenance in the next N days.
    pub fn is_maintenance_due_within(&self, days: i64) -> bool {
        self.next_maintenance_due
            .map(|d| {
                let days_until = (d - chrono::Local::now().date_naive()).num_days();
                days_until <= days
            })
            .unwrap_or(false)
    }
}
