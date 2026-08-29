//! POST /v1/internal/payments/intents — mesh-internal only (Istio mTLS gates
//! caller identity, same as every other route under /v1/internal). Callable
//! by order-intake to create a payment session for an amount order-intake has
//! already priced and verified — payments trusts the caller's amount here
//! specifically because this route is unreachable from any tenant-facing
//! credential, per the design spec's D3.

use axum::{extract::{Path, State}, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use logisticos_errors::AppError;

use crate::api::http::AppState;
use crate::application::services::payment_intent_service::CreateIntentCommand;
use crate::domain::repositories::payment_gateway::PaymentAction;

#[derive(Deserialize)]
pub struct CreateIntentRequest {
    pub tenant_id: Uuid,
    pub purpose: String,
    pub reference_type: String,
    pub reference_id: Uuid,
    pub amount_cents: i64,
    pub currency: String,
    pub return_url: String,
    /// `"sale"` (immediate capture) or `"authorize"` (ring-fence only,
    /// captured/voided later via the routes below). Absent defaults to
    /// `"sale"` — every caller that existed before this feature (and every
    /// caller that doesn't explicitly ask for authorize-then-capture) keeps
    /// getting the original behavior unchanged.
    #[serde(default)]
    pub action: Option<String>,
}

#[derive(Serialize)]
pub struct CreateIntentResponse {
    pub intent_id: Uuid,
    pub checkout_url: String,
}

pub async fn create_intent(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateIntentRequest>,
) -> Result<(StatusCode, Json<CreateIntentResponse>), AppError> {
    let payment_intent_service = state.payment_intent_service.as_ref().ok_or_else(|| {
        AppError::ServiceUnavailable(
            "Online card payment is not configured for this deployment (Network \
             International credentials are unset) — no payment intent can be created"
                .into(),
        )
    })?;
    let action = match req.action.as_deref() {
        None => PaymentAction::Sale,
        Some(s) => PaymentAction::parse(s).ok_or_else(|| {
            AppError::Validation(format!(
                "unknown payment action {s:?} — expected \"sale\" or \"authorize\""
            ))
        })?,
    };
    let created = payment_intent_service.create_intent(CreateIntentCommand {
        tenant_id: req.tenant_id,
        purpose: req.purpose,
        reference_type: req.reference_type,
        reference_id: req.reference_id,
        amount_cents: req.amount_cents,
        currency: req.currency,
        return_url: req.return_url,
        action,
    }).await?;

    Ok((
        StatusCode::CREATED,
        Json(CreateIntentResponse { intent_id: created.intent_id, checkout_url: created.checkout_url }),
    ))
}

/// `POST /v1/internal/payments/intents/:id/capture` — captures funds
/// previously ring-fenced by an `action: "authorize"` intent. Mesh-internal
/// only, same as `create_intent` above.
pub async fn capture_intent(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let payment_intent_service = state.payment_intent_service.as_ref().ok_or_else(|| {
        AppError::ServiceUnavailable(
            "Online card payment is not configured for this deployment (Network \
             International credentials are unset) — no payment intent can be captured"
                .into(),
        )
    })?;
    payment_intent_service.capture_intent(id).await?;
    Ok(StatusCode::OK)
}

/// `POST /v1/internal/payments/intents/:id/void` — releases an
/// authorization hold that was never captured (the no-courier path).
/// Mesh-internal only, same as `create_intent` above.
pub async fn void_intent(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let payment_intent_service = state.payment_intent_service.as_ref().ok_or_else(|| {
        AppError::ServiceUnavailable(
            "Online card payment is not configured for this deployment (Network \
             International credentials are unset) — no payment intent can be voided"
                .into(),
        )
    })?;
    payment_intent_service.void_intent(id).await?;
    Ok(StatusCode::OK)
}

#[cfg(test)]
mod tests {
    //! Router-level test: with Network International unconfigured, the real
    //! `POST /v1/internal/payments/intents` route must answer 503, not
    //! panic and not fall through to a 500.
    //!
    //! Builds the real `AppState` and the real `router()` — everything else
    //! in `AppState` is wired exactly as `bootstrap::run()` wires it, minus
    //! `payment_intent_service` (left `None`, the case under test). Kafka is
    //! real, not stubbed, over an in-process `rdkafka::mocking::MockCluster`
    //! — the same approach `payment_intent_service.rs`'s own test module
    //! uses, for the same reason: the services this state wires take a
    //! concrete `Arc<KafkaProducer>`, not a trait object. The `PgPool` is
    //! `connect_lazy` and never actually connects; that's fine because this
    //! route returns before any of these collaborators are ever touched.

    use std::sync::Arc;

    use axum::{body::Body, http::{Request, StatusCode}};
    use tower::ServiceExt as _;

    use logisticos_auth::jwt::JwtService;
    use logisticos_events::producer::KafkaProducer;

    use crate::api::http::{router, AppState};
    use crate::application::{
        queries::CommissionBreakdownQuery,
        services::{
            BillingAggregationService, CodRemittanceService, CodService, InvoiceService,
            WalletService, WithdrawalService,
        },
    };
    use crate::infrastructure::{
        cache::RedisSequenceSource,
        db::{
            PgBillingRunRepository, PgCodRemittanceBatchRepository, PgCodRepository,
            PgDriverLedgerRepository, PgInvoiceRepository, PgMerchantBillingAccountRepository,
            PgPartnerBonusRepo, PgWalletRepository, PgWithdrawalRequestRepository,
        },
        http::OrderIntakeClient,
    };

    fn test_kafka_producer() -> Arc<KafkaProducer> {
        use rdkafka::mocking::MockCluster;
        let cluster = MockCluster::new(1).expect("mock kafka cluster");
        let brokers = cluster.bootstrap_servers();
        // Leak the cluster so it outlives the producer for the duration of
        // this short-lived test process — same trade-off
        // `payment_intent_service.rs`'s identical helper makes.
        Box::leak(Box::new(cluster));
        Arc::new(KafkaProducer::new(&brokers).expect("kafka producer over mock cluster"))
    }

    /// Mirrors `bootstrap::run()`'s wiring for every `AppState` field this
    /// route's collaborators don't touch, with `payment_intent_service`
    /// fixed at `None` — the disabled-capability case under test.
    fn test_state_with_payment_disabled() -> Arc<AppState> {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://unused:unused@127.0.0.1:1/unused")
            .expect("lazy pool construction is infallible");
        let kafka = test_kafka_producer();

        let invoice_repo = Arc::new(PgInvoiceRepository::new(pool.clone()));
        let cod_repo = Arc::new(PgCodRepository::new(pool.clone()));
        let cod_batch_repo = Arc::new(PgCodRemittanceBatchRepository::new(pool.clone()));
        let wallet_repo = Arc::new(PgWalletRepository::new(pool.clone()));
        let withdrawal_repo = Arc::new(PgWithdrawalRequestRepository::new(pool.clone()));
        let billing_run_repo = Arc::new(PgBillingRunRepository::new(pool.clone()));
        let merchant_billing_account_repo = Arc::new(PgMerchantBillingAccountRepository::new(pool.clone()));
        let sequence_source = Arc::new(
            RedisSequenceSource::new("redis://127.0.0.1:1")
                .expect("redis::Client::open parses the URL but does not connect"),
        );
        let order_intake_client = Arc::new(OrderIntakeClient::new("http://127.0.0.1:1"));
        let driver_ledger_repo = Arc::new(PgDriverLedgerRepository::new(pool.clone()));
        let partner_bonus_repo = Arc::new(PgPartnerBonusRepo::new(pool.clone()));
        let commission_query = Arc::new(CommissionBreakdownQuery::new(pool.clone()));

        let invoice_service = Arc::new(InvoiceService::new(
            Arc::clone(&invoice_repo) as _,
            Arc::clone(&kafka),
            sequence_source as _,
            Arc::clone(&order_intake_client) as _,
        ));
        let cod_service = Arc::new(CodService::new(
            Arc::clone(&cod_repo) as _,
            Arc::clone(&order_intake_client) as _,
            Arc::clone(&kafka),
        ));
        let cod_remittance_service = Arc::new(CodRemittanceService::new(
            Arc::clone(&cod_repo) as _,
            Arc::clone(&cod_batch_repo) as _,
            Arc::clone(&wallet_repo) as _,
            Arc::clone(&kafka),
            Arc::clone(&merchant_billing_account_repo) as _,
        ));
        let wallet_service = Arc::new(WalletService::new(Arc::clone(&wallet_repo) as _));
        let withdrawal_service = Arc::new(WithdrawalService::new(
            Arc::clone(&wallet_repo) as _,
            Arc::clone(&withdrawal_repo),
            Arc::clone(&kafka),
        ));
        let billing_service = Arc::new(BillingAggregationService::new(
            Arc::clone(&billing_run_repo) as _,
            Arc::clone(&order_intake_client) as _,
            Arc::clone(&invoice_service),
        ));

        Arc::new(AppState {
            invoice_service,
            cod_service,
            cod_remittance_service,
            wallet_service,
            billing_service,
            jwt: Arc::new(JwtService::new("test-jwt-secret", 3600, 86400)),
            merchant_billing_account_repo: merchant_billing_account_repo as _,
            commission_query,
            partner_bonus_repo,
            withdrawal_service,
            pdf_renderer: None,
            driver_ledger_repo: driver_ledger_repo as _,
            payment_intent_service: None,
        })
    }

    #[tokio::test]
    async fn create_intent_returns_503_when_network_international_is_unconfigured() {
        let state = test_state_with_payment_disabled();
        let app = router(state);

        let body = serde_json::json!({
            "tenant_id": uuid::Uuid::new_v4(),
            "purpose": "shipping_fee",
            "reference_type": "shipment",
            "reference_id": uuid::Uuid::new_v4(),
            "amount_cents": 2_200,
            "currency": "AED",
            "return_url": "https://portal.test.local/payment/return",
        });

        let req = Request::builder()
            .method("POST")
            .uri("/v1/internal/payments/intents")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).expect("serialize body")))
            .expect("build request");

        let resp = app.oneshot(req).await.expect("oneshot request");
        assert_eq!(
            resp.status(),
            StatusCode::SERVICE_UNAVAILABLE,
            "an unconfigured NI gateway must surface as 503, not 500 and not a panic"
        );
    }

    #[tokio::test]
    async fn capture_intent_returns_503_when_network_international_is_unconfigured() {
        let state = test_state_with_payment_disabled();
        let app = router(state);

        let req = Request::builder()
            .method("POST")
            .uri(format!("/v1/internal/payments/intents/{}/capture", uuid::Uuid::new_v4()))
            .body(Body::empty())
            .expect("build request");

        let resp = app.oneshot(req).await.expect("oneshot request");
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn void_intent_returns_503_when_network_international_is_unconfigured() {
        let state = test_state_with_payment_disabled();
        let app = router(state);

        let req = Request::builder()
            .method("POST")
            .uri(format!("/v1/internal/payments/intents/{}/void", uuid::Uuid::new_v4()))
            .body(Body::empty())
            .expect("build request");

        let resp = app.oneshot(req).await.expect("oneshot request");
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}
