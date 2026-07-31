use std::{net::SocketAddr, sync::Arc};
use rdkafka::{consumer::StreamConsumer, producer::FutureProducer, ClientConfig};
use sqlx::postgres::PgPoolOptions;
use anyhow::Context as _;
use logisticos_auth::jwt::JwtService;

use crate::{
    application::services::{
        event_consumer::{handle_campaign_triggered, handle_hub_milestone, process_event, EngagementPublisher},
        notification_service::NotificationService,
    },
    config::Config,
    infrastructure::{
        cache::SuppressionCache,
        channels::{
            email::SesEmailAdapter,
            log_adapter::LogChannelAdapter,
            push::ExpoPushAdapter,
            sms::TwilioSmsAdapter,
            whatsapp::MetaWhatsAppAdapter,
            ChannelAdapter,
        },
        db::NotificationDb,
    },
};

// ---------------------------------------------------------------------------
// Kafka-backed engagement event publisher (for CAMPAIGN_COMPLETED)
// ---------------------------------------------------------------------------

struct KafkaPublisher {
    producer: FutureProducer,
}

#[async_trait::async_trait]
impl EngagementPublisher for KafkaPublisher {
    async fn publish(&self, topic: &str, key: &str, payload: &[u8]) -> anyhow::Result<()> {
        use rdkafka::producer::FutureRecord;
        use std::time::Duration;
        self.producer
            .send(
                FutureRecord::to(topic).key(key).payload(payload),
                Duration::from_secs(5),
            )
            .await
            .map_err(|(e, _)| anyhow::anyhow!("Kafka publish error: {}", e))?;
        Ok(())
    }
}

pub async fn run() -> anyhow::Result<()> {
    let cfg = Config::load()?;
    let otlp = std::env::var("OTLP_ENDPOINT").ok();
    logisticos_tracing::init(logisticos_tracing::TracingConfig {
        service_name: "engagement",
        env: &cfg.app.env,
        otlp_endpoint: otlp.as_deref(),
        log_level: None,
    })?;

    // Channel adapters — real when credentials present, LogChannelAdapter otherwise.
    // The log adapter prints the full rendered receipt to container stdout and
    // succeeds, making prod booking a verifiable end-to-end test without
    // paying Twilio/SES.
    let twilio_sid   = std::env::var("TWILIO_ACCOUNT_SID").ok();
    let twilio_token = std::env::var("TWILIO_AUTH_TOKEN").ok();
    let twilio_ready = twilio_sid.as_deref().is_some_and(is_real_cred)
        && twilio_token.as_deref().is_some_and(is_real_cred);

    // WhatsApp — Meta Cloud API (direct-to-Meta)
    let meta_token    = std::env::var("META_WHATSAPP_ACCESS_TOKEN").ok();
    let meta_phone_id = std::env::var("META_WHATSAPP_PHONE_NUMBER_ID").ok();
    let meta_ready    = meta_token.as_deref().is_some_and(is_real_cred)
        && meta_phone_id.as_deref().is_some_and(is_real_cred);

    let whatsapp: Arc<dyn ChannelAdapter> = if meta_ready {
        tracing::info!("engagement: WhatsApp using Meta Cloud API adapter");
        Arc::new(MetaWhatsAppAdapter::new(
            meta_token.unwrap(),
            meta_phone_id.unwrap(),
        ))
    } else {
        tracing::warn!("engagement: WhatsApp using LogChannelAdapter (META_WHATSAPP_* not set) — messages printed to stdout");
        Arc::new(LogChannelAdapter::new("whatsapp"))
    };

    let sms: Arc<dyn ChannelAdapter> = if twilio_ready {
        tracing::info!("engagement: SMS using Twilio adapter");
        Arc::new(TwilioSmsAdapter::new(
            twilio_sid.unwrap(),
            twilio_token.unwrap(),
            std::env::var("TWILIO_SMS_FROM").unwrap_or_else(|_| "+15005550006".into()),
        ))
    } else {
        tracing::warn!("engagement: SMS using LogChannelAdapter");
        Arc::new(LogChannelAdapter::new("sms"))
    };

    let email: Arc<dyn ChannelAdapter> = match std::env::var("SES_FROM_EMAIL").ok().as_deref() {
        Some(v) if is_real_cred(v) => {
            tracing::info!(from = %v, "engagement: Email using SES adapter");
            Arc::new(SesEmailAdapter::new(
                v.to_string(),
                std::env::var("SES_FROM_NAME").unwrap_or_else(|_| "CargoMarket".into()),
            ).await)
        }
        _ => {
            tracing::warn!("engagement: Email using LogChannelAdapter (SES_FROM_EMAIL not set)");
            Arc::new(LogChannelAdapter::new("email"))
        }
    };

    let push: Arc<dyn ChannelAdapter> = {
        let identity_base_url = std::env::var("SERVICES__IDENTITY_URL")
            .unwrap_or_else(|_| "http://identity:8001".into());
        Arc::new(ExpoPushAdapter::new(identity_base_url))
    };

    // Social / CRM channels — all stubbed via LogChannelAdapter until per-tenant
    // platform connectors (Meta Graph API, Telegram Bot API, Slack Web API, etc.)
    // are wired through the Connectors service. The log adapter succeeds and prints
    // the rendered message to stdout so campaigns flow end-to-end during dev/staging.
    let social: Arc<dyn ChannelAdapter> = Arc::new(LogChannelAdapter::new("social"));
    tracing::info!("engagement: Social channels (Messenger/Telegram/X/Viber/WeChat/Line/Slack) using LogChannelAdapter stub");

    let notification_svc = Arc::new(NotificationService::new(whatsapp, sms, email, push, social));

    // Database — used for campaign_sends tracking.
    let pool = PgPoolOptions::new()
        .max_connections(cfg.database.max_connections)
        .after_connect(|conn, _meta| Box::pin(async move {
            sqlx::query("SET search_path TO engagement, public")
                .execute(&mut *conn)
                .await?;
            Ok(())
        }))
        .connect(&cfg.database.url)
        .await?;

    logisticos_common::migrations::run(&pool, "engagement", &sqlx::migrate!("./migrations")).await?;
    let db = Arc::new(NotificationDb::new(pool.clone()));

    // Suppression cache — Redis-backed. Fails open if Redis is unavailable.
    let suppression_cache = Arc::new(
        match SuppressionCache::new(&cfg.redis.url).await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(err = %e, "engagement: Redis unavailable — campaign suppression disabled");
                SuppressionCache::new("redis://127.0.0.1/").await
                    .unwrap_or_else(|_| panic!("fallback redis client failed"))
            }
        }
    );

    // Kafka producer — publishes CAMPAIGN_COMPLETED after fan-out.
    let producer: FutureProducer = ClientConfig::new()
        .set("bootstrap.servers", &cfg.kafka.brokers)
        .set("message.timeout.ms", "5000")
        .create()?;
    let publisher = Arc::new(KafkaPublisher { producer });

    // Kafka consumer
    let consumer: Arc<StreamConsumer> = Arc::new(
        ClientConfig::new()
            .set("bootstrap.servers", &cfg.kafka.brokers)
            .set("group.id", &cfg.kafka.group_id)
            .set("auto.offset.reset", "earliest")
            .set("enable.auto.commit", "false")
            .create()?,
    );

    let consumer_svc       = notification_svc.clone();
    let consumer_cache     = suppression_cache.clone();
    let consumer_db        = db.clone();
    let consumer_publisher = publisher.clone();
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    tokio::spawn(async move {
        run_kafka_consumer(consumer, consumer_svc, consumer_cache, consumer_db, consumer_publisher, shutdown_rx).await;
    });

    // HTTP API — full REST router (templates, campaigns, notifications)
    let jwt_secret = std::env::var("AUTH__JWT_SECRET")
        .context("AUTH__JWT_SECRET env var not set")?;
    let jwt = Arc::new(JwtService::new(&jwt_secret, 3600, 86400));

    let http_state = crate::api::http::AppState {
        notification_svc: notification_svc.clone(),
        db: pool,
    };

    use tower_http::cors::CorsLayer;
    use axum::{http::{HeaderName, HeaderValue, Method}, Router};
    let cors = CorsLayer::new()
        .allow_origin([
            "http://localhost:3000".parse::<HeaderValue>().unwrap(),
            "http://localhost:3001".parse::<HeaderValue>().unwrap(),
            "http://localhost:3002".parse::<HeaderValue>().unwrap(),
            "http://localhost:3003".parse::<HeaderValue>().unwrap(),
        ])
        .allow_methods([
            Method::GET, Method::POST, Method::PUT,
            Method::PATCH, Method::DELETE, Method::OPTIONS,
        ])
        .allow_headers([
            HeaderName::from_static("content-type"),
            HeaderName::from_static("authorization"),
            HeaderName::from_static("x-logisticos-client"),
        ]);

    // Protected REST API — all routes require a valid JWT.
    let protected = crate::api::http::router(http_state.clone())
        .layer(axum::middleware::from_fn_with_state(
            jwt,
            logisticos_auth::middleware::require_auth,
        ));

    // Webhook routes — unauthenticated, verified by provider signatures.
    // Handles inbound WhatsApp messages, WhatsApp delivery/read receipts,
    // Twilio SMS status callbacks, and email open/click/bounce events.
    let webhook_state = crate::api::http::webhook::WebhookState {
        app_secret:   std::env::var("META_WHATSAPP_APP_SECRET").unwrap_or_default(),
        verify_token: std::env::var("META_WHATSAPP_VERIFY_TOKEN").unwrap_or_default(),
        twilio_token: std::env::var("TWILIO_AUTH_TOKEN").unwrap_or_default(),
        publisher:    publisher.clone(),
        db:           db.clone(),
    };
    let public_routes = crate::api::http::webhook::webhook_router(webhook_state);
    let internal_routes = crate::api::http::internal_router(http_state.clone());

    let app = Router::new()
        .merge(protected)
        .merge(public_routes)
        .merge(internal_routes)
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .layer(cors);

    let addr: SocketAddr = format!("{}:{}", cfg.app.host, cfg.app.port).parse()?;
    tracing::info!(addr = %addr, "engagement service listening");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    // Signal the Kafka consumer to stop
    shutdown_tx.send(true).ok();

    Ok(())
}

// ---------------------------------------------------------------------------
// HTTP router for direct notification dispatch
// ---------------------------------------------------------------------------

fn build_router(svc: Arc<NotificationService>) -> axum::Router {
    use axum::{extract::State, http::StatusCode, response::Json, routing::post, Router};
    use serde::Deserialize;
    use crate::domain::entities::notification::NotificationPriority;
    use crate::domain::entities::template::{NotificationChannel, NotificationTemplate};

    #[derive(Debug, Deserialize)]
    struct SendRequest {
        customer_id:  uuid::Uuid,
        tenant_id:    uuid::Uuid,
        channel:      String,
        template_id:  String,
        recipient:    String,
        variables:    serde_json::Value,
    }

    Router::new()
        .route("/v1/notifications", post(
            |State(svc): State<Arc<NotificationService>>, Json(req): Json<SendRequest>| async move {
                let channel = match req.channel.as_str() {
                    "whatsapp"  => NotificationChannel::WhatsApp,
                    "sms"       => NotificationChannel::Sms,
                    "email"     => NotificationChannel::Email,
                    "push"      => NotificationChannel::Push,
                    "messenger" => NotificationChannel::Messenger,
                    "telegram"  => NotificationChannel::Telegram,
                    "x"         => NotificationChannel::X,
                    "viber"     => NotificationChannel::Viber,
                    "wechat"    => NotificationChannel::WeChat,
                    "line"      => NotificationChannel::Line,
                    "slack"     => NotificationChannel::Slack,
                    _           => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "invalid channel"}))),
                };

                // Minimal template inline — production loads from DB template registry.
                let template = NotificationTemplate {
                    id:          uuid::Uuid::new_v4(),
                    tenant_id:   Some(req.tenant_id),
                    template_id: req.template_id.clone(),
                    channel,
                    language:    "en".into(),
                    subject:     None,
                    body:        req.variables.get("body").and_then(|v| v.as_str()).unwrap_or("{{body}}").to_owned(),
                    variables:   req.variables.as_object()
                        .map(|o| o.keys().cloned().collect())
                        .unwrap_or_default(),
                    is_active: true,
                };

                let mut notification = match NotificationService::build_from_template(
                    &template,
                    req.tenant_id,
                    req.customer_id,
                    req.recipient,
                    &req.variables,
                    NotificationPriority::Normal,
                ) {
                    Ok(n) => n,
                    Err(e) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e.to_string()}))),
                };

                match svc.dispatch(&mut notification).await {
                    Ok(_)  => (StatusCode::OK, Json(serde_json::json!({"id": notification.id, "status": "sent"}))),
                    Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))),
                }
            }
        ))
        .route("/health", axum::routing::get(|| async { (StatusCode::OK, "ok") }))
        .with_state(svc)
}

// ---------------------------------------------------------------------------
// Kafka consumer loop
// ---------------------------------------------------------------------------

async fn run_kafka_consumer(
    consumer:  Arc<StreamConsumer>,
    svc:       Arc<NotificationService>,
    cache:     Arc<SuppressionCache>,
    db:        Arc<NotificationDb>,
    publisher: Arc<KafkaPublisher>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    use rdkafka::{consumer::{CommitMode, Consumer}, Message};
    use logisticos_events::topics;

    consumer.subscribe(&[
        topics::SHIPMENT_CREATED,
        topics::DRIVER_ASSIGNED,
        topics::PICKUP_COMPLETED,
        topics::DELIVERY_COMPLETED,
        topics::DELIVERY_FAILED,
        topics::COD_COLLECTED,
        topics::COD_REMITTED,
        topics::WALLET_WITHDRAWAL_DISBURSED,
        topics::WALLET_WITHDRAWAL_REJECTED,
        topics::INVOICE_GENERATED,
        topics::RECEIPT_EMAIL_REQUESTED,
        topics::CAMPAIGN_TRIGGERED,
        topics::SUPPORT_TICKET_OPENED,
        topics::SUPPORT_TICKET_CLOSED,
        topics::CONTAINER_ARRIVED_AT_PORT,
        topics::CONTAINER_CUSTOMS_HOLD,
        topics::CONTAINER_CUSTOMS_CLEARED,
        topics::TENANT_FINALIZED,   // welcome email to new merchants
        topics::OTP_REQUESTED,      // email OTP delivery for passwordless login
    ]).expect("Engagement consumer subscription failed");

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow_and_update() {
                    tracing::info!("Engagement Kafka consumer shutting down");
                    break;
                }
            }
            result = consumer.recv() => {
                match result {
                    Ok(msg) => {
                        if let Some(payload) = msg.payload() {
                            if let Ok(json) = serde_json::from_slice::<serde_json::Value>(payload) {
                                let topic = msg.topic();
                                match topic {
                                    topics::SUPPORT_TICKET_OPENED => {
                                        handle_ticket_opened(&json, &cache).await;
                                    }
                                    topics::SUPPORT_TICKET_CLOSED => {
                                        handle_ticket_closed(&json, &cache).await;
                                    }
                                    topics::CAMPAIGN_TRIGGERED => {
                                        handle_campaign_triggered(
                                            &json,
                                            &db,
                                            &svc,
                                            &cache,
                                            publisher.as_ref(),
                                        ).await;
                                    }
                                    topics::CONTAINER_ARRIVED_AT_PORT
                                    | topics::CONTAINER_CUSTOMS_HOLD
                                    | topics::CONTAINER_CUSTOMS_CLEARED => {
                                        handle_hub_milestone(topic, &json, &svc, &cache).await;
                                    }
                                    _ => {
                                        process_event(topic, &json, &svc, &cache).await;
                                    }
                                }
                            }
                        }
                        consumer.commit_message(&msg, CommitMode::Async).ok();
                    }
                    Err(e) => {
                        tracing::error!(err = %e, "Engagement Kafka error");
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    }
                }
            }
        }
    }
}

async fn handle_ticket_opened(payload: &serde_json::Value, cache: &SuppressionCache) {
    let data = payload.get("data").unwrap_or(payload);
    let customer_id = data["customer_id"].as_str()
        .and_then(|s| s.parse::<uuid::Uuid>().ok());
    if let Some(id) = customer_id {
        if let Err(e) = cache.suppress(id).await {
            tracing::warn!(customer_id = %id, err = %e, "Failed to set campaign suppression flag");
        } else {
            tracing::info!(customer_id = %id, "Campaign suppression set — support ticket opened");
        }
    }
}

async fn handle_ticket_closed(payload: &serde_json::Value, cache: &SuppressionCache) {
    let data = payload.get("data").unwrap_or(payload);
    let customer_id = data["customer_id"].as_str()
        .and_then(|s| s.parse::<uuid::Uuid>().ok());
    if let Some(id) = customer_id {
        if let Err(e) = cache.lift(id).await {
            tracing::warn!(customer_id = %id, err = %e, "Failed to lift campaign suppression flag");
        } else {
            tracing::info!(customer_id = %id, "Campaign suppression lifted — support ticket closed");
        }
    }
}

/// Returns true if the env value looks like a real credential (not unset,
/// not empty, not one of our known placeholders).
fn is_real_cred(v: &str) -> bool {
    let t = v.trim();
    if t.is_empty() { return false; }
    !matches!(
        t,
        "dev-placeholder"
            | "placeholder"
            | "changeme"
            | "noreply@logisticos.app"
            | "noreply@cargomarket.net"
    )
}

async fn shutdown_signal() {
    use tokio::signal;
    let ctrl_c = async { signal::ctrl_c().await.expect("ctrl-c") };
    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate()).expect("SIGTERM").recv().await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = terminate => {} }
    tracing::info!("engagement shutdown");
}
