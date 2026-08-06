pub mod baskets;
pub mod catalog;
pub mod health;

use std::sync::Arc;
use axum::Router;

use crate::application::services::{BasketService, CatalogService};

pub struct AppState {
    pub catalog: Arc<CatalogService>,
    pub baskets: Arc<BasketService>,
    pub jwt:     Arc<logisticos_auth::jwt::JwtService>,
}

pub fn router(state: Arc<AppState>) -> Router {
    // Health is mounted outside the auth layer on purpose: container
    // healthchecks `curl -sf` these, and a 401 reads as a dead service. An open
    // incident on this platform has eight services showing red for eleven days
    // for exactly that reason.
    let auth_layer = axum::middleware::from_fn_with_state(
        Arc::clone(&state.jwt),
        logisticos_auth::middleware::require_auth,
    );

    Router::new()
        .merge(health::routes())
        .merge(
            catalog::routes()
                .merge(baskets::routes())
                .layer(auth_layer)
                .with_state(state),
        )
}
