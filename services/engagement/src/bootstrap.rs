use std::{net::SocketAddr, sync::Arc};
use rdkafka::{consumer::StreamConsumer, ClientConfig};
use sqlx::postgres::PgPoolOptions;

use crate::{
    application::services::{
        event_consumer::process_event,
        notification_service::NotificationService,
    },
    config::Config,
    infrastructure::channels::{
        email::SendGridEmailAdapter,
        sms::TwilioSmsAdapter,
        whatsapp::TwilioWhatsAppAdapter,
        ChannelAdapter,
    },
};

pub async fn run() -> anyhow::Result<()> {
    let cfg = Config::load()?;
    let otlp = std::env::var("OTLP_ENDPOINT").ok();
    logisticos_tracing::init(logisticos_tracing::TracingConfig {
        service_name: "engagement",
        env: &cfg.app.env,
        otlp_endpoint: otlp.as_deref(),
        log_level: None,
    })?;

    // Channel adapters — credentials from environment.
    // In dev, placeholders are accepted; actual sends will fail gracefully at the channel layer.
    let twilio_sid   = std::env::var("TWILIO_ACCOUNT_SID").unwrap_or_else(|_| "dev-placeholder".into());
    let twilio_token = std::env::var("TWILIO_AUTH_TOKEN").unwrap_or_else(|_| "dev-placeholder".into());
    let whatsapp: Arc<dyn ChannelAdapter> = Arc::new(TwilioWhatsAppAdapter::new(
        twilio_sid.clone(),
        twilio_token.clone(),
        std::env::var("TWILIO_WHATSAPP_FROM").unwrap_or_else(|_| "whatsapp:+15005550006".into()),
    ));
    let sms: Arc<dyn ChannelAdapter> = Arc::new(TwilioSmsAdapter::new(
        twilio_sid,
        twilio_token,
        std::env::var("TWILIO_SMS_FROM").unwrap_or_else(|_| "+15005550006".into()),
    ));
    let email: Arc<dyn ChannelAdapter> = Arc::new(SendGridEmailAdapter::new(
        std::env::var("SENDGRID_API_KEY").unwrap_or_else(|_| "dev-placeholder".into()),
        std::env::var("SENDGRID_FROM_EMAIL").unwrap_or_else(|_| "noreply@logisticos.dev".into()),
        std::env::var("SENDGRID_FROM_NAME").unwrap_or_else(|_| "LogisticOS".into()),
    ));

    let notification_svc = Arc::new(NotificationService::new(whatsapp, sms, email));

    // Kafka consumer
    let consumer: Arc<StreamConsumer> = Arc::new(
        ClientConfig::new()
            .set("bootstrap.servers", &cfg.kafka.brokers)
            .set("group.id", &cfg.kafka.group_id)
            .set("auto.offset.reset", "earliest")
            .set("enable.auto.commit", "false")
            .create()?,
    );

    let consumer_svc = notification_svc.clone();
    tokio::spawn(async move {
        run_kafka_consumer(consumer, consumer_svc).await;
    });

    // HTTP API — notification dispatch endpoint
    let app = build_router(notification_svc)
        .layer(tower_http::trace::TraceLayer::new_for_http());

    let addr: SocketAddr = format!("{}:{}", cfg.app.host, cfg.app.port).parse()?;
    tracing::info!(addr = %addr, "engagement service listening");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
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
                    "whatsapp" => NotificationChannel::WhatsApp,
                    "sms"      => NotificationChannel::Sms,
                    "email"    => NotificationChannel::Email,
                    "push"     => NotificationChannel::Push,
                    _          => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "invalid channel"}))),
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
    consumer: Arc<StreamConsumer>,
    svc: Arc<NotificationService>,
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
    ]).expect("Engagement consumer subscription failed");

    loop {
        match consumer.recv().await {
            Ok(msg) => {
                if let Some(payload) = msg.payload() {
                    if let Ok(json) = serde_json::from_slice::<serde_json::Value>(payload) {
                        process_event(msg.topic(), &json, &svc).await;
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
