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
    /// Broadcast channel for real-time location updates to WebSocket clients.
    pub location_tx: tokio::sync::broadcast::Sender<LocationBroadcast>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct LocationBroadcast {
    pub driver_id: uuid::Uuid,
    pub tenant_id: uuid::Uuid,
    pub lat: f64,
    pub lng: f64,
    pub heading: Option<f32>,
    pub speed_kmh: Option<f32>,
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
        .route("/drivers/:id",   get(drivers::get_driver))
        // Driver self-service (mobile app)
        .route("/drivers/me/online",  put(drivers::go_online))
        .route("/drivers/me/offline", put(drivers::go_offline))
        // Location updates from driver app
        .route("/location", post(location::update_location))
        // Task management
        .route("/tasks",          get(tasks::list_my_tasks))
        .route("/tasks/:id/start",    put(tasks::start_task))
        .route("/tasks/:id/complete", put(tasks::complete_task))
        .route("/tasks/:id/fail",     put(tasks::fail_task))
        // Admin/dispatcher overrides — ops can act on behalf of any driver.
        .route("/admin/tasks",                 get(tasks::admin_list_tasks))
        .route("/admin/tasks/:id/start",       put(tasks::admin_start_task))
        .route("/admin/tasks/:id/complete",    put(tasks::admin_complete_task))
        .layer(auth_layer)
}
