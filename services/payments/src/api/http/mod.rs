pub mod invoices;
pub mod wallet;
pub mod health;

use axum::{Router, routing::{get, post}};
use std::sync::Arc;
use crate::application::services::{InvoiceService, CodService, WalletService};

pub struct AppState {
    pub invoice_service: Arc<InvoiceService>,
    pub cod_service: Arc<CodService>,
    pub wallet_service: Arc<WalletService>,
    pub jwt: Arc<logisticos_auth::jwt::JwtService>,
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
        .route("/invoices",                              get(invoices::list_invoices).post(invoices::generate_invoice))
        .route("/invoices/:id",                          get(invoices::get_invoice))
        .route("/invoices/:id/resend",                   post(invoices::resend_invoice))
        .route("/customers/:customer_id/invoices",       get(invoices::list_customer_invoices))
        .route("/cod/reconcile",                         post(wallet::reconcile_cod))
        .route("/wallet",                                get(wallet::get_wallet))
        .route("/wallet/transactions",                   get(wallet::list_transactions))
        .route("/wallet/withdraw",                       post(wallet::request_withdrawal))
        .layer(auth_layer)
}
