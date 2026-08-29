pub mod invoices;
pub mod invoice_pdf;
pub mod driver_ledger;
pub mod wallet;
pub mod billing;
pub mod billing_clearance;
pub mod cod_batches;
pub mod health;
pub mod merchant_billing_accounts;
pub mod partner_commission;
pub mod withdrawal_requests;
pub mod payment_intents;
pub mod payment_webhooks;
pub mod subscriptions;

use axum::{Router, routing::{get, post}};
use std::sync::Arc;
use crate::application::services::{
    BillingAggregationService, CodRemittanceService, CodService, InvoiceService, WalletService,
};

pub struct AppState {
    pub invoice_service:                Arc<InvoiceService>,
    pub cod_service:                    Arc<CodService>,
    pub cod_remittance_service:         Arc<CodRemittanceService>,
    pub wallet_service:                 Arc<WalletService>,
    pub billing_service:                Arc<BillingAggregationService>,
    pub jwt:                            Arc<logisticos_auth::jwt::JwtService>,
    pub merchant_billing_account_repo:  Arc<dyn crate::domain::repositories::MerchantBillingAccountRepository>,
    pub commission_query:               Arc<crate::application::queries::CommissionBreakdownQuery>,
    pub partner_bonus_repo:             Arc<crate::infrastructure::db::partner_bonus_repo::PgPartnerBonusRepo>,
    pub withdrawal_service:             Arc<crate::application::services::WithdrawalService>,
    pub pdf_renderer:                   Option<Arc<crate::application::services::pdf_renderer::PdfRenderer>>,
    pub driver_ledger_repo:             Arc<dyn crate::domain::repositories::DriverLedgerRepository>,
    /// `None` when Network International is not configured for this
    /// deployment (see `Config::network_international`) — online card
    /// payment is an optional capability layered on top of the rest of this
    /// service. Every handler that needs it (`payment_intents::create_intent`,
    /// `payment_webhooks::network_international_webhook`) must check for
    /// `None` and return 503 rather than unwrapping.
    pub payment_intent_service:         Option<Arc<crate::application::services::payment_intent_service::PaymentIntentService>>,
    /// Always present, unlike `payment_intent_service`: the catalogue and the
    /// current-plan read work with no gateway configured, and only `checkout`
    /// needs one. It answers 503 for that one case rather than the whole
    /// subscription surface disappearing.
    pub subscription_service:           Arc<crate::application::services::SubscriptionService>,
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(health::health))
        .route("/ready",  get(health::ready))
        // Public webhook receiver — no JWT (NI's servers cannot hold a
        // LogisticOS session) and NOT nested under /v1/internal (mTLS-only),
        // since NI's servers live on the public internet. Authenticated
        // instead by NI's own webhook signature inside the handler.
        .route(
            "/v1/payments/webhooks/network-international",
            post(payment_webhooks::network_international_webhook),
        )
        // Internal service-to-service endpoints — no JWT (Istio mTLS gates caller identity).
        .nest("/v1/internal", internal_router(state.clone()))
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
        // Static segment declared before `/invoices/:id` so axum matches this
        // literally instead of treating "tenant" as an invoice id.
        .route("/invoices/tenant",                       get(invoices::list_tenant_invoices))
        .route("/invoices/:id",                          get(invoices::get_invoice))
        .route("/invoices/:id/pdf",                      get(invoice_pdf::download_invoice_pdf))
        .route("/invoices/:id/resend",                   post(invoices::resend_invoice))
        .route("/customers/:customer_id/invoices",       get(invoices::list_customer_invoices))
        .route("/cod/reconcile",                         post(wallet::reconcile_cod))
        .route("/cod/balance/:merchant_id",              get(wallet::get_cod_balance))
        .route("/cod/remittances",                       get(cod_batches::list_remittances))
        // Driver app: own COD cash position (Cash-to-Remit card + history)
        .route("/cod/driver-ledger/me",                  get(driver_ledger::get_my_ledger))
        .route("/wallet",                                get(wallet::get_wallet))
        .route("/wallet/transactions",                   get(wallet::list_transactions))
        .route("/wallet/withdrawal-requests",            get(wallet::list_withdrawal_requests))
        .route("/wallet/withdraw",                       post(wallet::request_withdrawal))
        .route(
            "/admin/merchants/:merchant_id/billing-account",
            get(merchant_billing_accounts::get_billing_account)
                .post(merchant_billing_accounts::upsert_billing_account)
                .patch(merchant_billing_accounts::patch_billing_account),
        )
        .route("/partner/commission/breakdown", get(partner_commission::get_commission_breakdown))
        .route("/admin/partner-bonuses",        post(partner_commission::create_partner_bonus))
        .route("/admin/billing/run",            post(billing::run_billing_admin))
        .route("/admin/withdrawal-requests",              get(withdrawal_requests::list_withdrawal_requests))
        .route("/admin/withdrawal-requests/:id/approve",  post(withdrawal_requests::approve_withdrawal))
        .route("/admin/withdrawal-requests/:id/disburse", post(withdrawal_requests::disburse_withdrawal))
        .route("/admin/withdrawal-requests/:id/reject",   post(withdrawal_requests::reject_withdrawal))
        // The tenant's own SaaS plan. Note what is absent: no route sets a
        // tier. See `subscriptions`' module doc.
        .route("/subscriptions/plans",     get(subscriptions::list_plans))
        .route("/subscriptions/me",        get(subscriptions::get_current))
        .route("/subscriptions/checkout",  post(subscriptions::checkout))
        .route("/subscriptions/me/cancel", post(subscriptions::cancel))
        .layer(auth_layer)
}

fn internal_router(_state: Arc<AppState>) -> Router<Arc<AppState>> {
    Router::new()
        .route("/billing/run",                  post(billing::run_billing))
        .route("/cod/batches",                  post(cod_batches::create_batch))
        .route("/cod/batches/:id/confirm",      post(cod_batches::confirm_batch))
        .route("/cod/balance/:merchant_id",     get(wallet::get_cod_balance_internal))
        // Billing clearance — called by hub-ops before container departure.
        // No JWT: Istio mTLS gates caller identity on /v1/internal/*.
        .route("/billing-clearance",            get(billing_clearance::get_billing_clearance))
        // Payment intent creation — called by order-intake for an amount it
        // has already priced and verified. No JWT: Istio mTLS gates caller
        // identity on /v1/internal/*.
        .route("/payments/intents",             post(payment_intents::create_intent))
        // Authorize-then-capture, with void: capture funds ring-fenced by an
        // action:"authorize" intent, or release the hold if nothing should
        // be charged. No JWT: Istio mTLS gates caller identity on /v1/internal/*.
        .route("/payments/intents/:id/capture", post(payment_intents::capture_intent))
        .route("/payments/intents/:id/void",    post(payment_intents::void_intent))
}
