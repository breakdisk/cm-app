use std::{net::SocketAddr, sync::Arc};
use rdkafka::{consumer::StreamConsumer, producer::FutureProducer, ClientConfig};
use sqlx::postgres::PgPoolOptions;

use crate::{
    application::services::{
        event_consumer::{handle_campaign_triggered, process_event, EngagementPublisher},
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
            whatsapp::TwilioWhatsAppAdapter,
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

    // Database — used for campaign_sends tracking and per-tenant channel config.
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

    let notification_svc = Arc::new(NotificationService::new(whatsapp, sms, email, push).with_db(pool.clone()));

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

    // HTTP API — full engagement API (templates, channel-configs, campaigns, notifications)
    let http_state = crate::api::http::AppState {
        notification_svc: notification_svc.clone(),
        db: pool.clone(),
    };
    let app = crate::api::http::router(http_state)
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
        topics::INVOICE_GENERATED,
        topics::RECEIPT_EMAIL_REQUESTED,
        topics::CAMPAIGN_TRIGGERED,
        topics::SUPPORT_TICKET_OPENED,
        topics::SUPPORT_TICKET_CLOSED,
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
