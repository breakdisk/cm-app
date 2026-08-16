pub mod couriers;
pub mod health;

use std::sync::Arc;
use axum::Router;

use crate::application::services::DispatchService;

pub struct AppState {
    pub dispatch: Arc<DispatchService>,
    pub jwt:      Arc<logisticos_auth::jwt::JwtService>,
}

pub fn router(state: Arc<AppState>) -> Router {
    // Health stays outside the auth layer: container healthchecks `curl -sf`
    // these, and a 401 reads as a dead service. An open incident on this
    // platform has eight services showing red for eleven days for exactly that.
    let auth_layer = axum::middleware::from_fn_with_state(
        Arc::clone(&state.jwt),
        logisticos_auth::middleware::require_auth,
    );

    Router::new()
        .merge(health::routes())
        .merge(couriers::routes().layer(auth_layer).with_state(state))
}
