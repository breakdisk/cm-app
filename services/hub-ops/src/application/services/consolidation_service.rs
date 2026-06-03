use std::sync::Arc;
use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::algorithms::bin_pack::{self, BoxItem, Bin};
use crate::domain::algorithms::dim_estimate::estimate_dims_cm;
use crate::domain::entities::consolidation::{ConsolidationPlan, TruckSpec};
use crate::infrastructure::hub_ws::HubBroadcaster;

// ── Repository traits ──────────────────────────────────────────────────────────

#[async_trait]
pub trait TruckSpecRepository: Send + Sync {
    async fn list(&self, tenant_id: Uuid) -> anyhow::Result<Vec<TruckSpec>>;
    async fn find(&self, id: Uuid, tenant_id: Uuid) -> anyhow::Result<Option<TruckSpec>>;
    async fn create(&self, spec: &TruckSpec) -> anyhow::Result<TruckSpec>;
    async fn update(&self, spec: &TruckSpec) -> anyhow::Result<TruckSpec>;
}

#[async_trait]
pub trait ConsolidationPlanRepository: Send + Sync {
    async fn list(&self, hub_id: Uuid, tenant_id: Uuid) -> anyhow::Result<Vec<ConsolidationPlan>>;
    async fn find(&self, id: Uuid, tenant_id: Uuid) -> anyhow::Result<Option<ConsolidationPlan>>;
    async fn upsert(&self, plan: &ConsolidationPlan) -> anyhow::Result<ConsolidationPlan>;
    async fn update_placements(
        &self,
        id: Uuid,
        tenant_id: Uuid,
        placements: serde_json::Value,
    ) -> anyhow::Result<ConsolidationPlan>;
}

// ── Commands / DTOs ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateSpecCommand {
    pub name:               String,
    pub transport_mode:     String,
    pub size_class:         String,
    pub interior_length_cm: i32,
    pub interior_width_cm:  i32,
    pub interior_height_cm: i32,
    pub max_payload_kg:     f64,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSpecCommand {
    pub name:               Option<String>,
    pub interior_length_cm: Option<i32>,
    pub interior_width_cm:  Option<i32>,
    pub interior_height_cm: Option<i32>,
    pub max_payload_kg:     Option<f64>,
    pub is_active:          Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ComputePlanCommand {
    pub hub_id:        Uuid,
    pub truck_spec_id: Uuid,
    /// Raw item list — missing dims (0) are estimated server-side.
    pub items:         Vec<BoxItemInput>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct BoxItemInput {
    pub awb:       String,
    pub weight_g:  u32,
    pub length_cm: u32,
    pub width_cm:  u32,
    pub height_cm: u32,
}

#[derive(Debug, Deserialize)]
pub struct UpdatePlacementsBody {
    pub placements: serde_json::Value,
}

// ── Service ────────────────────────────────────────────────────────────────────

pub struct ConsolidationService {
    specs:       Arc<dyn TruckSpecRepository>,
    plans:       Arc<dyn ConsolidationPlanRepository>,
    broadcaster: HubBroadcaster,
}

impl ConsolidationService {
    pub fn new(
        specs:       Arc<dyn TruckSpecRepository>,
        plans:       Arc<dyn ConsolidationPlanRepository>,
        broadcaster: HubBroadcaster,
    ) -> Self {
        Self { specs, plans, broadcaster }
    }

    pub async fn list_specs(&self, tenant_id: Uuid) -> anyhow::Result<Vec<TruckSpec>> {
        self.specs.list(tenant_id).await
    }

    pub async fn create_spec(
        &self,
        tenant_id: Uuid,
        cmd: CreateSpecCommand,
    ) -> anyhow::Result<TruckSpec> {
        let spec = TruckSpec {
            id:                 Uuid::new_v4(),
            tenant_id,
            name:               cmd.name,
            transport_mode:     cmd.transport_mode,
            size_class:         cmd.size_class,
            interior_length_cm: cmd.interior_length_cm,
            interior_width_cm:  cmd.interior_width_cm,
            interior_height_cm: cmd.interior_height_cm,
            max_payload_kg:     cmd.max_payload_kg,
            is_active:          true,
            created_at:         Utc::now(),
            updated_at:         Utc::now(),
        };
        self.specs.create(&spec).await
    }

    pub async fn update_spec(
        &self,
        id: Uuid,
        tenant_id: Uuid,
        cmd: UpdateSpecCommand,
    ) -> anyhow::Result<TruckSpec> {
        let mut spec = self.specs.find(id, tenant_id).await?
            .ok_or_else(|| anyhow::anyhow!("truck spec not found"))?;

        if let Some(v) = cmd.name               { spec.name = v; }
        if let Some(v) = cmd.interior_length_cm  { spec.interior_length_cm = v; }
        if let Some(v) = cmd.interior_width_cm   { spec.interior_width_cm = v; }
        if let Some(v) = cmd.interior_height_cm  { spec.interior_height_cm = v; }
        if let Some(v) = cmd.max_payload_kg      { spec.max_payload_kg = v; }
        if let Some(v) = cmd.is_active           { spec.is_active = v; }
        spec.updated_at = Utc::now();

        self.specs.update(&spec).await
    }

    pub async fn compute_plan(
        &self,
        tenant_id: Uuid,
        cmd: ComputePlanCommand,
    ) -> anyhow::Result<ConsolidationPlan> {
        let spec = self.specs.find(cmd.truck_spec_id, tenant_id).await?
            .ok_or_else(|| anyhow::anyhow!("truck spec not found"))?;

        // Resolve missing dimensions via DIM factor.
        let bin_items: Vec<BoxItem> = cmd.items.iter().map(|i| {
            let (l, w, h, estimated) = if i.length_cm == 0 || i.width_cm == 0 || i.height_cm == 0 {
                let (el, ew, eh) = estimate_dims_cm(i.weight_g);
                (el, ew, eh, true)
            } else {
                (i.length_cm, i.width_cm, i.height_cm, false)
            };
            BoxItem { awb: i.awb.clone(), weight_g: i.weight_g, length_cm: l, width_cm: w, height_cm: h, estimated }
        }).collect();

        let bin = Bin {
            length_cm:    spec.interior_length_cm as u32,
            width_cm:     spec.interior_width_cm as u32,
            height_cm:    spec.interior_height_cm as u32,
            max_payload_g: (spec.max_payload_kg * 1000.0) as u64,
        };

        let result = bin_pack::pack(&bin, &bin_items);

        let items_json     = serde_json::to_value(&bin_items)?;
        let placements_json = serde_json::to_value(&result.placements)?;
        let unplaced_json  = serde_json::to_value(&result.unplaced)?;

        let plan = ConsolidationPlan {
            id:               Uuid::new_v4(),
            tenant_id,
            hub_id:           cmd.hub_id,
            truck_spec_id:    cmd.truck_spec_id,
            container_id:     None,
            items:            items_json,
            placements:       placements_json,
            unplaced:         unplaced_json,
            total_weight_kg:  result.total_weight_kg,
            volume_used_cm3:  result.volume_used_cm3 as i64,
            volume_total_cm3: result.volume_total_cm3 as i64,
            piece_count:      result.placements.len() as i32,
            status:           "draft".to_owned(),
            loaded_awbs:      vec![],
            computed_at:      Utc::now(),
            created_at:       Utc::now(),
            updated_at:       Utc::now(),
        };

        let saved = self.plans.upsert(&plan).await?;

        // Broadcast plan_computed event to all WS subscribers for this hub.
        let event = serde_json::json!({
            "type": "plan_computed",
            "plan_id": saved.id,
            "hub_id": cmd.hub_id,
            "volume_pct": saved.volume_used_pct(),
            "weight_kg": saved.total_weight_kg,
            "piece_count": saved.piece_count,
            "unplaced_count": result.unplaced.len(),
        });
        self.broadcaster.broadcast(cmd.hub_id, event.to_string()).await;

        Ok(saved)
    }

    pub async fn get_plan(
        &self,
        id: Uuid,
        tenant_id: Uuid,
    ) -> anyhow::Result<Option<ConsolidationPlan>> {
        self.plans.find(id, tenant_id).await
    }

    pub async fn list_plans(
        &self,
        hub_id: Uuid,
        tenant_id: Uuid,
    ) -> anyhow::Result<Vec<ConsolidationPlan>> {
        self.plans.list(hub_id, tenant_id).await
    }

    pub async fn update_placements(
        &self,
        id: Uuid,
        tenant_id: Uuid,
        body: UpdatePlacementsBody,
    ) -> anyhow::Result<ConsolidationPlan> {
        let plan = self.plans.find(id, tenant_id).await?
            .ok_or_else(|| anyhow::anyhow!("plan not found"))?;

        let updated = self.plans.update_placements(id, tenant_id, body.placements).await?;

        // Broadcast drag-and-drop update.
        let event = serde_json::json!({
            "type": "placements_updated",
            "plan_id": id,
            "hub_id": plan.hub_id,
        });
        self.broadcaster.broadcast(plan.hub_id, event.to_string()).await;

        Ok(updated)
    }
}
