pub mod drivers;
pub mod tasks;
pub mod location;
pub mod health;
pub mod ws;

use axum::{Router, routing::{get, post, put}};
use std::sync::Arc;
use crate::application::services::{DriverService, TaskService, LocationService};

pub struct AppState {
    pub driver_service:   Arc<DriverService>,
    pub task_service:     Arc<TaskService>,
    pub location_service: Arc<LocationService>,
    pub jwt: Arc<logisticos_auth::jwt::JwtService>,
    /// Broadcast channel for real-time roster updates (location + status) to WebSocket clients.
    pub roster_tx: tokio::sync::broadcast::Sender<RosterEvent>,
}

/// Events fanned out to WebSocket subscribers. Tenant-scoped on the server side —
/// the `tenant_id()` getter is used for filtering.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RosterEvent {
    LocationUpdated {
        driver_id: uuid::Uuid,
        tenant_id: uuid::Uuid,
        lat: f64,
        lng: f64,
        heading: Option<f32>,
        speed_kmh: Option<f32>,
    },
    StatusChanged {
        driver_id: uuid::Uuid,
        tenant_id: uuid::Uuid,
        status: String,
        is_online: bool,
        active_route_id: Option<uuid::Uuid>,
    },
}

impl RosterEvent {
    pub fn tenant_id(&self) -> uuid::Uuid {
        match self {
            RosterEvent::LocationUpdated { tenant_id, .. } => *tenant_id,
            RosterEvent::StatusChanged   { tenant_id, .. } => *tenant_id,
        }
    }
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(health::health))
        .route("/ready",  get(health::ready))
        // WebSocket for live driver tracking — no auth middleware (uses token query param)
        .route("/ws/locations", get(ws::handle_ws_upgrade))
        .nest("/v1", protected_router(state.clone()))
        .with_state(state)
}

fn protected_router(state: Arc<AppState>) -> Router<Arc<AppState>> {
    let auth_layer = axum::middleware::from_fn_with_state(
        Arc::clone(&state.jwt),
        logisticos_auth::middleware::require_auth,
    );
    Router::new()
        // Drivers (fleet management — dispatcher role)
        .route("/drivers",       get(drivers::list_drivers).post(drivers::register_driver))
        .route("/drivers/:id",   get(drivers::get_driver).patch(drivers::update_driver))
        // Admin override: force a driver's status (FLEET_MANAGE permission)
        .route("/drivers/:id/status", put(drivers::set_driver_status))
        // Driver self-service (mobile app) — flat paths avoid matchit ambiguity with /:id
        .route("/drivers/go-online",  post(drivers::go_online))
        .route("/drivers/go-offline", post(drivers::go_offline))
        // Location updates from driver app
        .route("/location", post(location::update_location))
        // Task management
        .route("/tasks",          get(tasks::list_my_tasks))
        // Aggregated manifest — partner portal daily view. Must come
        // before /:id patterns to avoid matchit picking up "manifest"
        // as a task id.
        .route("/tasks/manifest", get(tasks::list_manifest))
        .route("/tasks/:id/start",    put(tasks::start_task))
        .route("/tasks/:id/complete", put(tasks::complete_task))
        .route("/tasks/:id/fail",     put(tasks::fail_task))
        .layer(auth_layer)
}
