pub mod drivers;
pub mod tasks;
pub mod location;
pub mod health;
pub mod ws;

use axum::{Router, routing::{delete, get, post, put}};
use std::sync::Arc;
use crate::application::services::{DriverService, TaskService, LocationService};
use crate::infrastructure::external::FcmClient;

pub struct AppState {
    pub driver_service:   Arc<DriverService>,
    pub task_service:     Arc<TaskService>,
    pub location_service: Arc<LocationService>,
    pub jwt: Arc<logisticos_auth::jwt::JwtService>,
    /// Broadcast channel for real-time roster updates (location + status) to WebSocket clients.
    pub roster_tx: tokio::sync::broadcast::Sender<RosterEvent>,
    /// Optional FCM client for push notifications. None when FCM env vars are unset.
    pub fcm: Option<Arc<FcmClient>>,
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
    /// Sent when an admin dispatches an instruction to a driver via
    /// `POST /v1/drivers/:id/instructions`. Surfaced to the dispatch console
    /// so the UI can show an "instruction sent" toast without polling.
    Instruction {
        driver_id:        uuid::Uuid,
        tenant_id:        uuid::Uuid,
        instruction_type: String,
        message:          String,
    },
}

impl RosterEvent {
    pub fn tenant_id(&self) -> uuid::Uuid {
        match self {
            RosterEvent::LocationUpdated { tenant_id, .. } => *tenant_id,
            RosterEvent::StatusChanged   { tenant_id, .. } => *tenant_id,
            RosterEvent::Instruction     { tenant_id, .. } => *tenant_id,
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
        // Static sub-paths must be declared before /:id to prevent matchit capturing them as ids.
        .route("/drivers",              get(drivers::list_drivers).post(drivers::register_driver))
        .route("/drivers/summary",      get(drivers::get_summary))
        .route("/drivers/me",           get(drivers::get_me_driver))
        // Static sub-path before /drivers/:id — same matchit ordering rule.
        .route("/drivers/me/earnings",  get(tasks::my_earnings))
        .route("/drivers/go-online",    post(drivers::go_online))
        .route("/drivers/go-offline",   post(drivers::go_offline))
        .route("/drivers/:id",          get(drivers::get_driver).patch(drivers::update_driver).delete(drivers::delete_driver))
        // Admin override: force a driver's status (FLEET_MANAGE permission)
        .route("/drivers/:id/status",        put(drivers::set_driver_status))
        // Admin override: cancel all pending/in-progress tasks for a driver
        .route("/drivers/:id/cancel-tasks",  post(drivers::cancel_driver_tasks))
        // MCP tool endpoint: get live location for a specific driver
        .route("/drivers/:id/location",      get(drivers::get_driver_location))
        // MCP tool endpoint: send an operational instruction to a driver via FCM + WS
        .route("/drivers/:id/instructions",  post(drivers::send_driver_instruction))
        // Admin: task history for any driver by UUID
        .route("/drivers/:id/tasks/history", get(tasks::admin_list_driver_task_history))
        // Location updates from driver app
        .route("/location", post(location::update_location))
        // Task management
        .route("/tasks",           get(tasks::list_my_tasks))
        // Aggregated manifest — partner portal daily view. Must come
        // before /:id patterns to avoid matchit picking up "manifest"
        // as a task id.
        .route("/tasks/manifest",  get(tasks::list_manifest))
        // Completed + failed tasks for the current driver (history view)
        .route("/tasks/history",   get(tasks::list_task_history))
        .route("/tasks/:id/start",    put(tasks::start_task))
        .route("/tasks/:id/complete", put(tasks::complete_task))
        .route("/tasks/:id/fail",     put(tasks::fail_task))
        .layer(auth_layer)
}
