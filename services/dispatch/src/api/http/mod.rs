pub mod routes;
pub mod assignments;
pub mod health;
pub mod queue;
pub mod dispatch_ops;
pub mod offers;

use axum::{Router, routing::{get, post, put}};
use std::sync::Arc;
use crate::application::services::{DriverAssignmentService, OfferService};
use crate::infrastructure::db::{DispatchQueueRepository, DriverProfilesRepository};

pub struct AppState {
    pub dispatch_service: Arc<DriverAssignmentService>,
    pub offer_service:    Arc<OfferService>,
    pub jwt:              Arc<logisticos_auth::jwt::JwtService>,
    pub queue_repo:       Arc<dyn DispatchQueueRepository>,
    pub drivers_repo:     Arc<dyn DriverProfilesRepository>,
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(health::health))
        .route("/ready",  get(health::ready))
        // Internal service-to-service endpoints — no JWT required.
        // Istio mTLS gates caller identity within the service mesh.
        .route("/v1/internal/shipments/:shipment_id/requeue", post(dispatch_ops::requeue_shipment))
        .route("/v1/internal/shipments/:shipment_id/assign",  post(dispatch_ops::internal_assign))
        .route("/v1/internal/drivers/available",              get(dispatch_ops::internal_available_drivers))
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
        // Admin: cancel a planned/in-progress route
        .route("/routes/:id/cancel",   put(routes::cancel_route))
        // Auto-assign the best available driver to a route
        .route("/routes/:id/assign",   post(assignments::auto_assign))
        // Driver actions — called from mobile app
        .route("/assignments/:id/accept", put(assignments::accept))
        .route("/assignments/:id/reject", put(assignments::reject))
        // Dispatch queue and driver roster
        .route("/queue",   get(queue::list_queue))
        .route("/drivers", get(queue::list_drivers))
        // Gig offer broadcast ("grab") — driver-facing surface
        .route("/offers/open",      get(offers::list_open))
        .route("/offers/:id/claim", post(offers::claim))
        .route("/offers/:id/pass",  post(offers::pass))
        .route("/offers/:id/seen",  post(offers::seen))
        // Quick dispatch — one-shot assign driver to shipment in queue
        .route("/queue/:shipment_id/dispatch",        post(dispatch_ops::quick_dispatch))
        // Ops-console action: broadcast to the gig pool instead of 1:1 dispatch
        .route("/queue/:shipment_id/broadcast",       post(offers::broadcast))
        // Admin: cancel a dispatched shipment — returns it to pending for reassignment
        .route("/queue/:shipment_id/cancel-dispatch", post(dispatch_ops::cancel_queue_dispatch))
        // Admin: cancel a driver's stale active assignment (re-enters them into auto-dispatch pool)
        .route("/drivers/:id/cancel-assignment",      post(dispatch_ops::cancel_driver_assignment))
        .layer(auth_layer)
}
