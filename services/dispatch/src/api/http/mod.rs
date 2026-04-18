pub mod routes;
pub mod assignments;
pub mod health;
pub mod queue;
pub mod dispatch_ops;

use axum::{Router, routing::{get, post, put}};
use std::sync::Arc;
use crate::application::services::DriverAssignmentService;
use crate::infrastructure::db::{DispatchQueueRepository, DriverProfilesRepository};

pub struct AppState {
    pub dispatch_service: Arc<DriverAssignmentService>,
    pub jwt:              Arc<logisticos_auth::jwt::JwtService>,
    pub queue_repo:       Arc<dyn DispatchQueueRepository>,
    pub drivers_repo:     Arc<dyn DriverProfilesRepository>,
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(health::health))
        .route("/ready",  get(health::ready))
        .nest("/v1", protected_router(state.clone()))
        .with_state(state)
}

fn protected_router(state: Arc<AppState>) -> Router<Arc<AppState>> {
    let auth_layer = axum::middleware::from_fn_with_state(
        Arc::clone(&state.jwt),
        logisticos_auth::middleware::require_auth,
    );
    Router::new()
        // Route management
        .route("/routes",              get(routes::list_routes).post(routes::create_route))
        .route("/routes/:id",          get(routes::get_route))
        // Auto-assign the best available driver to a route
        .route("/routes/:id/assign",   post(assignments::auto_assign))
        // Driver actions — called from mobile app
        .route("/assignments/:id/accept", put(assignments::accept))
        .route("/assignments/:id/reject", put(assignments::reject))
        // Dispatch queue and driver roster
        .route("/queue",   get(queue::list_queue))
        .route("/drivers", get(queue::list_drivers))
        // Quick dispatch — one-shot assign driver to shipment in queue
        .route("/queue/:shipment_id/dispatch", post(dispatch_ops::quick_dispatch))
        .layer(auth_layer)
}
