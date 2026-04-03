use std::sync::Arc;
use std::net::SocketAddr;

use rdkafka::{
    consumer::{CommitMode, Consumer, StreamConsumer},
    producer::FutureProducer,
    ClientConfig, Message,
};
use serde_json::Value;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

use crate::{
    api::http::{router, AppState},
    application::services::{
        build_context, execute_actions, ActionExecutor, RuleRepository,
    },
    config::Config,
    domain::entities::rule::failed_delivery_rule,
    infrastructure::db::PgRuleRepository,
};

// ---------------------------------------------------------------------------
// HTTP client-backed ActionExecutor
// ---------------------------------------------------------------------------

struct HttpActionExecutor {
    http:           reqwest::Client,
    engagement_url: String,
    order_url:      String,
    producer:       FutureProducer,
}

#[async_trait::async_trait]
impl ActionExecutor for HttpActionExecutor {
    async fn notify_customer(
        &self,
        _tenant_id: Uuid,
        customer_id: Uuid,
        channel: &str,
        template_id: &str,
        ctx: &crate::domain::entities::rule::RuleContext,
    ) -> anyhow::Result<()> {
        self.http
            .post(format!("{}/v1/notifications", self.engagement_url))
            .json(&serde_json::json!({
                "customer_id":  customer_id,
                "tenant_id":    ctx.tenant_id,
                "channel":      channel,
                "template_id":  template_id,
                "recipient":    "",
                "variables":    ctx.metadata,
            }))
            .send()
            .await?;
        Ok(())
    }

    async fn reschedule_delivery(&self, shipment_id: Uuid, delay_hours: u32) -> anyhow::Result<()> {
        self.http
            .post(format!("{}/v1/shipments/{}/reschedule", self.order_url, shipment_id))
            .json(&serde_json::json!({"delay_hours": delay_hours}))
            .send()
            .await?;
        Ok(())
    }

    async fn alert_dispatcher(&self, tenant_id: Uuid, message: &str, priority: &str) -> anyhow::Result<()> {
        tracing::warn!(tenant_id = %tenant_id, priority = %priority, "Dispatcher alert: {}", message);
        Ok(())
    }

    async fn emit_event(&self, topic: &str, key: &str, payload: &[u8]) -> anyhow::Result<()> {
        use rdkafka::producer::FutureRecord;
        self.producer
            .send(
                FutureRecord::to(topic).key(key).payload(payload),
                std::time::Duration::from_secs(5),
            )
            .await
            .map_err(|(e, _)| anyhow::anyhow!("Kafka publish error: {}", e))?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Bootstrap
// ---------------------------------------------------------------------------

pub async fn run() -> anyhow::Result<()> {
    let cfg = Config::load()?;
    let otlp = std::env::var("OTLP_ENDPOINT").ok();
    logisticos_tracing::init(logisticos_tracing::TracingConfig {
        service_name: "business-logic",
        env: &cfg.app.env,
        otlp_endpoint: otlp.as_deref(),
        log_level: None,
    })?;

    // Database pool — used for rule persistence.
    let pool = PgPoolOptions::new()
        .max_connections(cfg.database.max_connections)
        .connect(&cfg.database.url)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;

    let pg_repo = Arc::new(PgRuleRepository::new(pool));

    // Seed in-memory engine from DB + platform defaults.
    let mut initial_rules = pg_repo.load_all().await?;
    // Always include the platform failed-delivery rule if not already persisted.
    let platform_rule_id = Uuid::nil();
    if !initial_rules.iter().any(|r| r.tenant_id == platform_rule_id) {
        initial_rules.push(failed_delivery_rule(Uuid::nil()));
    }
    let rule_repo = Arc::new(RuleRepository::new(initial_rules));

    let jwt_secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| "change-me-in-production".into());
    let jwt = Arc::new(logisticos_auth::jwt::JwtService::new(
        &jwt_secret,
        3600,
        86400,
    ));

    let producer: FutureProducer = ClientConfig::new()
        .set("bootstrap.servers", &cfg.kafka.brokers)
        .set("message.timeout.ms", "5000")
        .create()?;

    let executor = Arc::new(HttpActionExecutor {
        http:           reqwest::Client::new(),
        engagement_url: std::env::var("ENGAGEMENT_URL").unwrap_or_else(|_| "http://engagement:8010".into()),
        order_url:      std::env::var("ORDER_INTAKE_URL").unwrap_or_else(|_| "http://order-intake:8003".into()),
        producer,
    });

    let consumer: Arc<StreamConsumer> = Arc::new(
        ClientConfig::new()
            .set("bootstrap.servers", &cfg.kafka.brokers)
            .set("group.id", &cfg.kafka.group_id)
            .set("auto.offset.reset", "earliest")
            .set("enable.auto.commit", "false")
            .create()?,
    );

    consumer.subscribe(&[
        "logisticos.order.shipment.created",
        "logisticos.driver.delivery.failed",
        "logisticos.driver.delivery.completed",
        "logisticos.driver.delivery.attempted",
        "logisticos.marketing.campaign.triggered",
    ])?;

    // HTTP server: health + rules CRUD API.
    let state = AppState {
        rule_repo:  rule_repo.clone(),
        pg_repo:    pg_repo.clone(),
        jwt:        jwt.clone(),
    };

    let app = router()
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", cfg.app.host, cfg.app.port).parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(addr = %addr, "business-logic rules engine + API listening");

    tokio::select! {
        result = axum::serve(listener, app) => {
            result?;
        }
        _ = run_consumer(consumer, rule_repo, executor, pg_repo) => {}
    }

    Ok(())
}

async fn run_consumer(
    consumer: Arc<StreamConsumer>,
    rules: Arc<RuleRepository>,
    executor: Arc<dyn ActionExecutor>,
    pg_repo: Arc<PgRuleRepository>,
) {
    loop {
        match consumer.recv().await {
            Ok(msg) => {
                let tenant_id = extract_tenant_id(&msg).unwrap_or(Uuid::nil());
                let topic = msg.topic().to_owned();

                if let Some(payload) = msg.payload() {
                    if let Ok(json) = serde_json::from_slice::<Value>(payload) {
                        let matching_rules = rules.rules_for_topic(tenant_id, &topic).await;
                        let ctx = build_context(tenant_id, &topic, &json);

                        for rule in matching_rules {
                            if rule.conditions_met(&ctx) {
                                tracing::info!(rule_id = %rule.id, rule_name = %rule.name, topic = %topic, "Rule fired");

                                let outcome = match execute_actions(&rule, &ctx, executor.as_ref()).await {
                                    Ok(()) => "success",
                                    Err(e) => {
                                        tracing::error!(rule_id = %rule.id, err = %e, "Rule action failed");
                                        "error"
                                    }
                                };

                                pg_repo.log_execution(
                                    rule.id,
                                    ctx.tenant_id,
                                    &topic,
                                    ctx.shipment_id,
                                    true,
                                    &rule.actions.iter().map(|a| format!("{:?}", a)).collect::<Vec<_>>(),
                                    outcome,
                                    None,
                                    chrono::Utc::now(),
                                ).await.ok();
                            }
                        }
                    }
                }

                consumer.commit_message(&msg, CommitMode::Async).ok();
            }
            Err(e) => {
                tracing::error!(err = %e, "Business-logic Kafka recv error");
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        }
    }
}

fn extract_tenant_id(msg: &rdkafka::message::BorrowedMessage<'_>) -> Option<Uuid> {
    use rdkafka::message::Headers;
    msg.headers().and_then(|headers| {
        headers.iter().find_map(|h| {
            if h.key == "tenant_id" {
                h.value.and_then(|v| std::str::from_utf8(v).ok()).and_then(|s| s.parse().ok())
            } else {
                None
            }
        })
    })
}
