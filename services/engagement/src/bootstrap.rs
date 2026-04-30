use std::{net::SocketAddr, sync::Arc};
use rdkafka::{consumer::StreamConsumer, ClientConfig};

use crate::{
    application::services::{
        event_consumer::process_event,
        notification_service::NotificationService,
    },
    config::Config,
    infrastructure::channels::{
        email::SesEmailAdapter,
        log_adapter::LogChannelAdapter,
        push::ExpoPushAdapter,
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

    // Channel adapters — real when credentials present, LogChannelAdapter otherwise.
    // The log adapter prints the full rendered receipt to container stdout and
    // succeeds, making prod booking a verifiable end-to-end test without
    // paying Twilio/SES.
    let twilio_sid   = std::env::var("TWILIO_ACCOUNT_SID").ok();
    let twilio_token = std::env::var("TWILIO_AUTH_TOKEN").ok();
    let twilio_ready = twilio_sid.as_deref().is_some_and(is_real_cred)
        && twilio_token.as_deref().is_some_and(is_real_cred);

    let whatsapp: Arc<dyn ChannelAdapter> = if twilio_ready {
        tracing::info!("engagement: WhatsApp using Twilio adapter");
        Arc::new(TwilioWhatsAppAdapter::new(
            twilio_sid.clone().unwrap(),
            twilio_token.clone().unwrap(),
            std::env::var("TWILIO_WHATSAPP_FROM").unwrap_or_else(|_| "whatsapp:+15005550006".into()),
        ))
    } else {
        tracing::warn!("engagement: WhatsApp using LogChannelAdapter (TWILIO_* not set) — receipts printed to stdout");
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

    let notification_svc = Arc::new(NotificationService::new(whatsapp, sms, email, push));

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
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    tokio::spawn(async move {
        run_kafka_consumer(consumer, consumer_svc, shutdown_rx).await;
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
        topics::INVOICE_GENERATED,
        topics::RECEIPT_EMAIL_REQUESTED,
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
