use std::sync::Arc;
use logisticos_errors::{AppError, AppResult};
use logisticos_types::{Coordinates, DriverId, RouteId, TenantId, VehicleId};
use logisticos_events::{producer::KafkaProducer, topics, envelope::Event};
use tokio::sync::Mutex;
use uuid::Uuid;
use crate::infrastructure::db::{ComplianceCache, DispatchQueueRepository};

use crate::{
    application::commands::{
        CreateRouteCommand, AutoAssignDriverCommand, AcceptAssignmentCommand,
        RejectAssignmentCommand, RouteView, QuickDispatchCommand,
    },
    domain::{
        entities::{Route, DeliveryStop, DriverAssignment, StopType, RouteStatus},
        events::{RouteCreated, DriverAssigned},
        repositories::{RouteRepository, DriverAssignmentRepository, DriverAvailabilityRepository},
        value_objects::{MAX_STOPS_BUSINESS, DEFAULT_DRIVER_SEARCH_RADIUS_KM, estimate_duration_minutes},
    },
};

pub struct DriverAssignmentService {
    route_repo: Arc<dyn RouteRepository>,
    assignment_repo: Arc<dyn DriverAssignmentRepository>,
    driver_avail_repo: Arc<dyn DriverAvailabilityRepository>,
    kafka: Arc<KafkaProducer>,
    compliance_cache: Option<Arc<Mutex<ComplianceCache>>>,
    queue_repo: Arc<dyn DispatchQueueRepository>,
}

impl DriverAssignmentService {
    pub fn new(
        route_repo: Arc<dyn RouteRepository>,
        assignment_repo: Arc<dyn DriverAssignmentRepository>,
        driver_avail_repo: Arc<dyn DriverAvailabilityRepository>,
        kafka: Arc<KafkaProducer>,
        compliance_cache: Option<Arc<Mutex<ComplianceCache>>>,
        queue_repo: Arc<dyn DispatchQueueRepository>,
    ) -> Self {
        Self { route_repo, assignment_repo, driver_avail_repo, kafka, compliance_cache, queue_repo }
    }

    /// Create a new route for a driver. The route starts as Planned and gets stops added.
    /// Caller is responsible for verifying the driver is available before calling.
    pub async fn create_route(
        &self,
        tenant_id: TenantId,
        cmd: CreateRouteCommand,
    ) -> AppResult<Route> {
        // Validate driver doesn't already have an active route
        let driver_id = DriverId::from_uuid(cmd.driver_id);
        let existing = self.route_repo
            .find_active_by_driver(&driver_id).await
            .map_err(AppError::Internal)?;

        if existing.is_some() {
            return Err(AppError::BusinessRule(
                "Driver already has an active route — complete or cancel it first".into()
            ));
        }

        let route = Route {
            id: RouteId::new(),
            tenant_id: tenant_id.clone(),
            driver_id: driver_id.clone(),
            vehicle_id: VehicleId::from_uuid(cmd.vehicle_id),
            stops: Vec::new(),
            status: RouteStatus::Planned,
            total_distance_km: 0.0,
            estimated_duration_minutes: 0,
            created_at: chrono::Utc::now(),
            started_at: None,
            completed_at: None,
        };

        self.route_repo.save(&route).await.map_err(AppError::Internal)?;

        let event = Event::new("dispatch", "route.created", tenant_id.inner(), RouteCreated {
            route_id: route.id.inner(),
            tenant_id: tenant_id.inner(),
            driver_id: driver_id.inner(),
            stop_count: 0,
            total_distance_km: 0.0,
        });
        self.kafka.publish_event(topics::ROUTE_CREATED, &event).await
            .map_err(AppError::Internal)?;

        tracing::info!(route_id = %route.id, driver_id = %driver_id, "Route created");
        Ok(route)
    }

    /// Auto-assign the optimal driver to a route using proximity-based scoring.
    ///
    /// Algorithm:
    /// 1. Find all online, unassigned drivers within `DEFAULT_DRIVER_SEARCH_RADIUS_KM`
    ///    of the route's first stop.
    /// 2. Score by: distance (70%) + active stop load (30%).
    /// 3. Assign the highest-scoring driver; fall back to explicit `preferred_driver_id`.
    pub async fn auto_assign_driver(
        &self,
        tenant_id: TenantId,
        cmd: AutoAssignDriverCommand,
    ) -> AppResult<DriverAssignment> {
        let route_id = RouteId::from_uuid(cmd.route_id);
        let route = self.route_repo.find_by_id(&route_id).await.map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound { resource: "Route", id: cmd.route_id.to_string() })?;

        if route.status != RouteStatus::Planned {
            return Err(AppError::BusinessRule("Can only assign drivers to Planned routes".into()));
        }

        if route.stops.is_empty() {
            return Err(AppError::BusinessRule("Cannot assign driver to a route with no stops".into()));
        }

        // Anchor point = first stop's coordinates
        let anchor = route.stops[0].address.coordinates
            .ok_or_else(|| AppError::BusinessRule("Route first stop has no GPS coordinates".into()))?;

        let driver_id = if let Some(preferred_id) = cmd.preferred_driver_id {
            // Dispatcher explicitly chose a driver — still validate they're available
            DriverId::from_uuid(preferred_id)
        } else {
            // Find nearest available driver automatically
            let candidates = self.driver_avail_repo
                .find_available_near(&tenant_id, anchor, DEFAULT_DRIVER_SEARCH_RADIUS_KM)
                .await
                .map_err(AppError::Internal)?;

            if candidates.is_empty() {
                return Err(AppError::BusinessRule(format!(
                    "No available drivers within {}km of the first stop",
                    DEFAULT_DRIVER_SEARCH_RADIUS_KM
                )));
            }

            // Score: lower is better. Formula: distance_km * 0.7 + stop_load * 0.3
            // This biases toward proximity while penalizing overloaded drivers.
            let best = candidates.iter()
                .min_by(|a, b| {
                    let score_a = a.distance_km * 0.7 + a.active_stop_count as f64 * 0.3;
                    let score_b = b.distance_km * 0.7 + b.active_stop_count as f64 * 0.3;
                    score_a.partial_cmp(&score_b).unwrap_or(std::cmp::Ordering::Equal)
                })
                .unwrap(); // Safe: checked is_empty() above

            tracing::info!(
                driver_id = %best.driver_id,
                distance_km = %best.distance_km,
                stop_load = %best.active_stop_count,
                "Auto-selected driver for route"
            );

            best.driver_id.clone()
        };

        // Compliance gate — fail fast if driver is not cleared for assignment.
        // On cache miss we default to assignable; the compliance service corrects within TTL.
        // On Redis error we also default to assignable but log so operators can detect outages.
        // compliance_cache is None in test environments — skip the check entirely.
        let is_assignable = if let Some(ref cc) = self.compliance_cache {
            let mut cache = cc.lock().await;
            match cache.get_status(driver_id.inner()).await {
                Ok(Some((_, assignable))) => assignable,
                Ok(None)                  => true, // cache miss — assume compliant
                Err(e) => {
                    tracing::warn!(
                        driver_id = %driver_id,
                        "Compliance cache read error — defaulting to assignable: {e}"
                    );
                    true
                }
            }
        } else {
            true
        };

        if !is_assignable {
            return Err(AppError::BusinessRule(format!(
                "Driver {driver_id} is not compliance-cleared for assignment"
            )));
        }

        // Check driver doesn't already have a pending/accepted assignment
        let existing = self.assignment_repo
            .find_active_by_driver(&driver_id).await
            .map_err(AppError::Internal)?;

        if existing.is_some() {
            return Err(AppError::BusinessRule(
                "Selected driver already has an active assignment".into()
            ));
        }

        let assignment = DriverAssignment::new(tenant_id.clone(), driver_id.clone(), route_id.clone());
        self.assignment_repo.save(&assignment).await.map_err(AppError::Internal)?;

        // Emit one event per delivery stop so each customer is notified
        // of their driver assignment (with their customer_id for engagement service)
        for stop in &route.stops {
            // Look up customer_id from dispatch_queue for this shipment
            let customer_id = self.queue_repo
                .find_by_shipment(stop.shipment_id)
                .await
                .ok()
                .flatten()
                .map(|row| row.customer_id)
                .unwrap_or_else(|| {
                    tracing::warn!(
                        shipment_id = %stop.shipment_id,
                        "Could not find shipment in dispatch_queue — customer_id unavailable"
                    );
                    Uuid::nil()
                });

            let event = Event::new("dispatch", "driver.assigned", tenant_id.inner(), DriverAssigned {
                assignment_id: assignment.id,
                shipment_id: stop.shipment_id,
                customer_id,
                route_id: route_id.inner(),
                driver_id: driver_id.inner(),
                tenant_id: tenant_id.inner(),
            });
            self.kafka.publish_event(topics::DRIVER_ASSIGNED, &event).await
                .map_err(AppError::Internal)?;
        }

        tracing::info!(
            assignment_id = %assignment.id,
            driver_id = %driver_id,
            route_id = %route_id,
            "Driver assignment created"
        );

        Ok(assignment)
    }

    /// Driver accepts their assignment — transitions route to InProgress.
    pub async fn accept_assignment(
        &self,
        driver_id: &DriverId,
        cmd: AcceptAssignmentCommand,
    ) -> AppResult<()> {
        let mut assignment = self.assignment_repo
            .find_by_id(cmd.assignment_id).await.map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound { resource: "Assignment", id: cmd.assignment_id.to_string() })?;

        // Verify this driver owns the assignment — prevent cross-driver tampering
        if &assignment.driver_id != driver_id {
            return Err(AppError::Forbidden { resource: "Assignment".into() });
        }

        assignment.accept()
            .map_err(|e| AppError::BusinessRule(e.to_string()))?;

        // Transition route to InProgress
        let mut route = self.route_repo.find_by_id(&assignment.route_id).await.map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound { resource: "Route", id: assignment.route_id.inner().to_string() })?;

        route.status = RouteStatus::InProgress;
        route.started_at = Some(chrono::Utc::now());

        self.assignment_repo.save(&assignment).await.map_err(AppError::Internal)?;
        self.route_repo.save(&route).await.map_err(AppError::Internal)?;

        tracing::info!(assignment_id = %assignment.id, route_id = %route.id, "Driver accepted route");
        Ok(())
    }

    /// Driver rejects the assignment — marks it rejected; dispatcher must re-assign.
    pub async fn reject_assignment(
        &self,
        driver_id: &DriverId,
        cmd: RejectAssignmentCommand,
    ) -> AppResult<()> {
        let mut assignment = self.assignment_repo
            .find_by_id(cmd.assignment_id).await.map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound { resource: "Assignment", id: cmd.assignment_id.to_string() })?;

        if &assignment.driver_id != driver_id {
            return Err(AppError::Forbidden { resource: "Assignment".into() });
        }

        assignment.reject(cmd.reason)
            .map_err(|e| AppError::BusinessRule(e.to_string()))?;

        self.assignment_repo.save(&assignment).await.map_err(AppError::Internal)?;

        tracing::info!(
            assignment_id = %assignment.id,
            reason = ?assignment.rejection_reason,
            "Driver rejected assignment"
        );
        Ok(())
    }

    pub async fn list_routes(&self, tenant_id: &TenantId) -> AppResult<Vec<RouteView>> {
        let routes = self.route_repo.list_by_tenant(tenant_id).await.map_err(AppError::Internal)?;
        Ok(routes.into_iter().map(|r| RouteView {
            route_id: r.id.inner(),
            driver_id: r.driver_id.inner(),
            vehicle_id: r.vehicle_id.inner(),
            status: format!("{:?}", r.status),
            stop_count: r.stops.len(),
            total_distance_km: r.total_distance_km,
            estimated_duration_minutes: r.estimated_duration_minutes,
            created_at: r.created_at.to_rfc3339(),
        }).collect())
    }

    pub async fn get_route(&self, route_id: &RouteId) -> AppResult<Route> {
        self.route_repo.find_by_id(route_id).await.map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound { resource: "Route", id: route_id.inner().to_string() })
    }

    /// Returns available drivers near the given coordinates for MCP tool use.
    pub async fn list_available_drivers(
        &self,
        tenant_id: &TenantId,
        anchor: Coordinates,
        radius_km: f64,
    ) -> AppResult<Vec<crate::domain::repositories::AvailableDriver>> {
        self.driver_avail_repo
            .find_available_near(tenant_id, anchor, radius_km)
            .await
            .map_err(AppError::Internal)
    }

    /// One-shot dispatch: find shipment in queue → create route → assign driver →
    /// emit TASK_ASSIGNED + DRIVER_ASSIGNED → mark queue item dispatched.
    pub async fn quick_dispatch(
        &self,
        tenant_id: TenantId,
        cmd: QuickDispatchCommand,
    ) -> AppResult<DriverAssignment> {
        use logisticos_events::{payloads::TaskAssigned, topics};

        // 1. Load shipment from queue
        let queue_item = self.queue_repo
            .find_by_shipment(cmd.shipment_id)
            .await
            .map_err(|e| AppError::Internal(e.into()))?
            .ok_or_else(|| AppError::NotFound {
                resource: "Shipment in dispatch queue",
                id: cmd.shipment_id.to_string(),
            })?;

        if queue_item.status != "pending" {
            return Err(AppError::BusinessRule(format!(
                "Shipment {} is already {} — cannot dispatch again",
                cmd.shipment_id, queue_item.status
            )));
        }

        // 2. Find driver (explicit or auto-selected by proximity score)
        let driver_id = match cmd.preferred_driver_id {
            Some(id) => DriverId::from_uuid(id),
            None => {
                // Use origin (pickup point) as anchor for driver proximity — the driver
                // needs to travel to the origin first. Fall back to destination if no origin.
                // Error explicitly if neither is set — a NULL-coord default (previously Manila)
                // silently routed every undispatchable shipment to a false "no drivers nearby"
                // response, hiding the real cause: the shipment was never geocoded on create.
                let anchor = match (
                    queue_item.origin_lat.or(queue_item.dest_lat),
                    queue_item.origin_lng.or(queue_item.dest_lng),
                ) {
                    (Some(lat), Some(lng)) => Coordinates { lat, lng },
                    _ => return Err(AppError::BusinessRule(
                        "Shipment has no origin/destination coordinates — cannot dispatch. \
                         Ensure the merchant address is geocoded before booking.".into(),
                    )),
                };
                let candidates = self.driver_avail_repo
                    .find_available_near(&tenant_id, anchor, DEFAULT_DRIVER_SEARCH_RADIUS_KM)
                    .await
                    .map_err(AppError::Internal)?;

                if candidates.is_empty() {
                    return Err(AppError::BusinessRule("No available drivers nearby".into()));
                }

                candidates.iter()
                    .min_by(|a, b| {
                        let sa = a.distance_km * 0.7 + a.active_stop_count as f64 * 0.3;
                        let sb = b.distance_km * 0.7 + b.active_stop_count as f64 * 0.3;
                        sa.partial_cmp(&sb).unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .unwrap() // safe: checked is_empty() above
                    .driver_id
                    .clone()
            }
        };

        // 3. Compliance gate (skipped in test environments where compliance_cache is None)
        let is_assignable = if let Some(ref cc) = self.compliance_cache {
            let mut cache = cc.lock().await;
            match cache.get_status(driver_id.inner()).await {
                Ok(Some((_, assignable))) => assignable,
                Ok(None) => true,
                Err(e) => {
                    tracing::warn!(
                        driver_id = %driver_id,
                        "Compliance cache error — defaulting to assignable: {e}"
                    );
                    true
                }
            }
        } else {
            true
        };
        if !is_assignable {
            return Err(AppError::BusinessRule(format!(
                "Driver {driver_id} is not compliance-cleared for assignment"
            )));
        }

        // 4a. Guard: driver must not already have an active assignment
        let existing = self.assignment_repo
            .find_active_by_driver(&driver_id).await
            .map_err(AppError::Internal)?;
        if existing.is_some() {
            return Err(AppError::BusinessRule(
                "Selected driver already has an active assignment".into()
            ));
        }

        // 4. Create a minimal single-stop route (vehicle_id = nil, stop added by driver-ops)
        let route_id = RouteId::new();
        let route = Route {
            id: route_id.clone(),
            tenant_id: tenant_id.clone(),
            driver_id: driver_id.clone(),
            vehicle_id: VehicleId::from_uuid(Uuid::nil()),
            stops: vec![],
            status: RouteStatus::Planned,
            total_distance_km: 0.0,
            estimated_duration_minutes: 0,
            created_at: chrono::Utc::now(),
            started_at: None,
            completed_at: None,
        };
        self.route_repo.save(&route).await.map_err(AppError::Internal)?;

        // 5. Create assignment
        let assignment = DriverAssignment::new(tenant_id.clone(), driver_id.clone(), route_id.clone());
        self.assignment_repo.save(&assignment).await.map_err(AppError::Internal)?;

        // 6. Emit TASK_ASSIGNED twice — once for the pickup leg (origin) and once
        //    for the delivery leg (destination). driver-ops creates one DriverTask
        //    row per event, sequenced so the driver app shows pickup → delivery.
        //    Both events share the same assignment_id so they belong to the same
        //    job. We only emit the pickup task when origin coordinates exist —
        //    legacy shipments without structured origin fall through to a single
        //    delivery task (pre-Path-A behavior).
        let has_origin = !queue_item.origin_address_line1.is_empty()
            || queue_item.origin_lat.is_some();

        let mut next_sequence: i32 = 1;
        if has_origin {
            let pickup_task_id = Uuid::new_v4();
            let pickup_event = Event::new("dispatch", "task.assigned", tenant_id.inner(), TaskAssigned {
                task_id:              pickup_task_id,
                assignment_id:        assignment.id,
                shipment_id:          cmd.shipment_id,
                route_id:             route_id.inner(),
                driver_id:            driver_id.inner(),
                tenant_id:            tenant_id.inner(),
                sequence:             next_sequence,
                task_type:            "pickup".into(),
                address_line1:        queue_item.origin_address_line1.clone(),
                address_city:         queue_item.origin_city.clone(),
                address_province:     queue_item.origin_province.clone(),
                address_postal_code:  queue_item.origin_postal_code.clone(),
                address_lat:          queue_item.origin_lat,
                address_lng:          queue_item.origin_lng,
                customer_name:        queue_item.customer_name.clone(),
                customer_phone:       queue_item.customer_phone.clone(),
                cod_amount_cents:     None,                    // COD is collected on delivery, never on pickup
                special_instructions: queue_item.special_instructions.clone(),
                tracking_number:      queue_item.tracking_number.clone().unwrap_or_default(),
                customer_email:       queue_item.customer_email.clone().unwrap_or_default(),
            });
            self.kafka.publish_event(topics::TASK_ASSIGNED, &pickup_event).await
                .map_err(AppError::Internal)?;
            next_sequence += 1;
        }

        let delivery_task_id = Uuid::new_v4();
        let delivery_event = Event::new("dispatch", "task.assigned", tenant_id.inner(), TaskAssigned {
            task_id:              delivery_task_id,
            assignment_id:        assignment.id,
            shipment_id:          cmd.shipment_id,
            route_id:             route_id.inner(),
            driver_id:            driver_id.inner(),
            tenant_id:            tenant_id.inner(),
            sequence:             next_sequence,
            task_type:            "delivery".into(),
            address_line1:        queue_item.dest_address_line1.clone(),
            address_city:         queue_item.dest_city.clone(),
            address_province:     queue_item.dest_province.clone(),
            address_postal_code:  queue_item.dest_postal_code.clone(),
            address_lat:          queue_item.dest_lat,
            address_lng:          queue_item.dest_lng,
            customer_name:        queue_item.customer_name.clone(),
            customer_phone:       queue_item.customer_phone.clone(),
            cod_amount_cents:     queue_item.cod_amount_cents,
            special_instructions: queue_item.special_instructions.clone(),
            tracking_number:      queue_item.tracking_number.clone().unwrap_or_default(),
            customer_email:       queue_item.customer_email.clone().unwrap_or_default(),
        });
        self.kafka.publish_event(topics::TASK_ASSIGNED, &delivery_event).await
            .map_err(AppError::Internal)?;

        // 7. Emit DRIVER_ASSIGNED — engagement service sends push notification with customer_id
        let driver_assigned_event = Event::new("dispatch", "driver.assigned", tenant_id.inner(), DriverAssigned {
            assignment_id: assignment.id,
            shipment_id:   cmd.shipment_id,
            customer_id:   queue_item.customer_id,  // For engagement service notification routing
            route_id:      route_id.inner(),
            driver_id:     driver_id.inner(),
            tenant_id:     tenant_id.inner(),
        });
        self.kafka.publish_event(topics::DRIVER_ASSIGNED, &driver_assigned_event).await
            .map_err(AppError::Internal)?;

        // 8. Mark queue item as dispatched
        self.queue_repo.mark_dispatched(cmd.shipment_id).await
            .map_err(|e| AppError::Internal(e.into()))?;

        tracing::info!(
            shipment_id = %cmd.shipment_id,
            driver_id   = %driver_id,
            assignment_id = %assignment.id,
            "Quick dispatch complete"
        );
        Ok(assignment)
    }

    /// Admin operation: cancel any active (`pending`/`accepted`) assignment
    /// for the given driver, re-entering them into the auto-dispatch pool.
    /// Returns `true` if an assignment was cancelled.
    pub async fn admin_cancel_driver_assignment(
        &self,
        driver_id: DriverId,
        tenant_id: &TenantId,
    ) -> AppResult<bool> {
        let cancelled = self.assignment_repo
            .cancel_active_for_driver(&driver_id)
            .await
            .map_err(AppError::Internal)?;

        tracing::info!(
            driver_id = %driver_id,
            tenant_id = %tenant_id,
            cancelled,
            "Admin cancelled driver assignment"
        );
        Ok(cancelled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_assigned_event_has_required_fields() {
        // Validate that TaskAssigned payload compiles and has all fields
        let _ = logisticos_events::payloads::TaskAssigned {
            task_id:             uuid::Uuid::new_v4(),
            assignment_id:       uuid::Uuid::new_v4(),
            shipment_id:         uuid::Uuid::new_v4(),
            route_id:            uuid::Uuid::new_v4(),
            driver_id:           uuid::Uuid::new_v4(),
            tenant_id:           uuid::Uuid::new_v4(),
            sequence:            1i32,
            task_type:           "delivery".into(),
            address_line1:       "123 Test St".into(),
            address_city:        "Manila".into(),
            address_province:    "Metro Manila".into(),
            address_postal_code: "1000".into(),
            address_lat:         Some(14.5995),
            address_lng:         Some(120.9842),
            customer_name:       "Test Customer".into(),
            customer_phone:      "+63912345678".into(),
            cod_amount_cents:    None,
            special_instructions: None,
            tracking_number:     "CM-PH1-S0001234X".into(),
            customer_email:      "test@example.com".into(),
        };
    }
}
