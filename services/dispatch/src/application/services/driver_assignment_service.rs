use std::sync::Arc;
use logisticos_errors::{AppError, AppResult};
use logisticos_types::{Coordinates, DriverId, RouteId, TenantId, VehicleId};
use logisticos_events::{producer::KafkaProducer, topics, envelope::Event};
use tokio::sync::Mutex;
use uuid::Uuid;
use crate::infrastructure::db::ComplianceCache;

use crate::{
    application::commands::{
        CreateRouteCommand, AutoAssignDriverCommand, AcceptAssignmentCommand,
        RejectAssignmentCommand, RouteView,
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
    compliance_cache: Arc<Mutex<ComplianceCache>>,
}

impl DriverAssignmentService {
    pub fn new(
        route_repo: Arc<dyn RouteRepository>,
        assignment_repo: Arc<dyn DriverAssignmentRepository>,
        driver_avail_repo: Arc<dyn DriverAvailabilityRepository>,
        kafka: Arc<KafkaProducer>,
        compliance_cache: Arc<Mutex<ComplianceCache>>,
    ) -> Self {
        Self { route_repo, assignment_repo, driver_avail_repo, kafka, compliance_cache }
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
        let is_assignable = {
            let mut cache = self.compliance_cache.lock().await;
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

        let event = Event::new("dispatch", "driver.assigned", tenant_id.inner(), DriverAssigned {
            assignment_id: assignment.id,
            route_id: route_id.inner(),
            driver_id: driver_id.inner(),
            tenant_id: tenant_id.inner(),
        });
        self.kafka.publish_event(topics::DRIVER_ASSIGNED, &event).await
            .map_err(AppError::Internal)?;

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
}
