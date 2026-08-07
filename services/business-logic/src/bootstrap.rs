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
        build_context, enrich_segments, execute_actions, ActionExecutor, RuleRepository, SegmentChecker,
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
    dispatch_url:   String,
    marketing_url:  String,
    cdp_url:        String,
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
        // Resolve the customer's contact address from CDP before calling engagement.
        // The engagement service requires a non-empty recipient (phone or email).
        let token = std::env::var("SERVICES__CDP_TOKEN").unwrap_or_default();
        let profile_resp = self.http
            .get(format!("{}/v1/customers/{}", self.cdp_url, customer_id))
            .bearer_auth(&token)
            .header("X-Tenant-Id", ctx.tenant_id.to_string())
            .send()
            .await?;

        let recipient = if profile_resp.status().is_success() {
            let profile: serde_json::Value = profile_resp.json().await.unwrap_or_default();
            match channel {
                "email" => profile["email"].as_str().unwrap_or("").to_owned(),
                "sms"   => profile["phone"].as_str().unwrap_or("").to_owned(),
                _       => profile["phone"].as_str()
                    .or_else(|| profile["email"].as_str())
                    .unwrap_or("")
                    .to_owned(),
            }
        } else {
            String::new()
        };

        if recipient.is_empty() {
            anyhow::bail!(
                "notify_customer: no {} contact found for customer {} — skipping",
                channel, customer_id
            );
        }

        self.http
            .post(format!("{}/v1/notifications", self.engagement_url))
            .json(&serde_json::json!({
                "customer_id":  customer_id,
                "tenant_id":    ctx.tenant_id,
                "channel":      channel,
                "template_id":  template_id,
                "recipient":    recipient,
                "variables":    ctx.metadata,
            }))
            .send()
            .await?;
        Ok(())
    }

    async fn reschedule_delivery(&self, shipment_id: Uuid, _delay_hours: u32) -> anyhow::Result<()> {
        // Re-queue the shipment in dispatch so a new driver can be assigned.
        // The delay_hours param is advisory — the dispatch engine assigns the
        // next available driver immediately; ops can add scheduling later.
        let res = self.http
            .post(format!("{}/v1/internal/shipments/{}/requeue", self.dispatch_url, shipment_id))
            .send()
            .await?;
        if !res.status().is_success() && res.status().as_u16() != 204 {
            anyhow::bail!("requeue endpoint returned {}", res.status());
        }
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

    async fn trigger_campaign(
        &self,
        rule_id:     Uuid,
        rule_name:   &str,
        campaign_id: Uuid,
        ctx:         &crate::domain::entities::rule::RuleContext,
    ) -> anyhow::Result<()> {
        // Segment IDs were already resolved before conditions_met(); no action here.
        let Some(customer_id) = ctx.customer_id else {
            anyhow::bail!("TriggerCampaign: no customer_id in rule context — cannot resolve recipient");
        };
        let res = self.http
            .post(format!(
                "{}/v1/internal/campaigns/{}/trigger-for-recipient",
                self.marketing_url, campaign_id
            ))
            .json(&serde_json::json!({
                "customer_id":             customer_id,
                "tenant_id":               ctx.tenant_id,
                "rule_id":                 rule_id,
                "rule_name":               rule_name,
                "shipment_id":             ctx.shipment_id,
            }))
            .send()
            .await?;
        if !res.status().is_success() {
            let status = res.status();
            let body   = res.text().await.unwrap_or_default();
            anyhow::bail!("Marketing trigger-for-recipient returned {status}: {body}");
        }
        Ok(())
    }

    async fn assign_new_driver(&self, tenant_id: Uuid, shipment_id: Uuid) -> anyhow::Result<()> {
        let res = self.http
            .post(format!("{}/v1/internal/shipments/{}/assign", self.dispatch_url, shipment_id))
            .json(&serde_json::json!({ "tenant_id": tenant_id }))
            .send()
            .await?;
        if !res.status().is_success() {
            let status = res.status();
            let body   = res.text().await.unwrap_or_default();
            anyhow::bail!("dispatch internal assign returned {status}: {body}");
        }
        Ok(())
    }

    async fn update_shipment_status(&self, tenant_id: Uuid, shipment_id: Uuid, status: &str) -> anyhow::Result<()> {
        let res = self.http
            .put(format!("{}/v1/internal/shipments/{}/status", self.order_url, shipment_id))
            .json(&serde_json::json!({
                "tenant_id": tenant_id,
                "status":    status,
                "reason":    "automation rule",
            }))
            .send()
            .await?;
        if !res.status().is_success() && res.status().as_u16() != 204 {
            let status_code = res.status();
            let body        = res.text().await.unwrap_or_default();
            anyhow::bail!("order-intake internal status update returned {status_code}: {body}");
        }
        Ok(())
    }

    async fn webhook_call(&self, url: &str, method: &str, ctx: &crate::domain::entities::rule::RuleContext) -> anyhow::Result<()> {
        let http_method = reqwest::Method::from_bytes(method.to_uppercase().as_bytes())
            .map_err(|_| anyhow::anyhow!("WebhookCall: invalid HTTP method '{}'", method))?;
        let res = self.http
            .request(http_method, url)
            .json(&serde_json::json!({
                "tenant_id":    ctx.tenant_id,
                "event_type":   ctx.event_type,
                "shipment_id":  ctx.shipment_id,
                "customer_id":  ctx.customer_id,
                "metadata":     ctx.metadata,
            }))
            .send()
            .await?;
        if !res.status().is_success() {
            let status = res.status();
            let body   = res.text().await.unwrap_or_default();
            anyhow::bail!("WebhookCall to {} returned {status}: {body}", url);
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl SegmentChecker for HttpActionExecutor {
    async fn segment_ids_for_customer(
        &self,
        tenant_id: Uuid,
        customer_id: Uuid,
    ) -> anyhow::Result<Vec<Uuid>> {
        // Uses the internal service token stored in env rather than a user JWT.
        let token = std::env::var("SERVICES__CDP_TOKEN").unwrap_or_default();
        let resp = self.http
            .get(format!("{}/v1/segments/membership", self.cdp_url))
            .bearer_auth(&token)
            .header("X-Tenant-Id", tenant_id.to_string())
            .query(&[("customer_id", customer_id.to_string())])
            .send()
            .await?;

        if !resp.status().is_success() {
            anyhow::bail!("CDP membership endpoint returned {}", resp.status());
        }

        #[derive(serde::Deserialize)]
        struct Resp { segment_ids: Vec<Uuid> }
        let body: Resp = resp.json().await?;
        Ok(body.segment_ids)
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
        .after_connect(|conn, _meta| Box::pin(async move {
            sqlx::query("SET search_path TO business_logic, public")
                .execute(&mut *conn)
                .await?;
            Ok(())
        }))
        .connect(&cfg.database.url)
        .await?;

    logisticos_common::migrations::run(&pool, "business_logic", &sqlx::migrate!("./migrations")).await?;

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

    let cdp_url = std::env::var("CDP_URL").unwrap_or_else(|_| "http://cdp:8005".into());
    let executor = Arc::new(HttpActionExecutor {
        http:           reqwest::Client::new(),
        engagement_url: std::env::var("ENGAGEMENT_URL").unwrap_or_else(|_| "http://engagement:8010".into()),
        order_url:      std::env::var("ORDER_INTAKE_URL").unwrap_or_else(|_| "http://order-intake:8003".into()),
        dispatch_url:   std::env::var("DISPATCH_URL").unwrap_or_else(|_| "http://dispatch:8004".into()),
        marketing_url:  std::env::var("MARKETING_URL").unwrap_or_else(|_| "http://marketing:8012".into()),
        cdp_url:        cdp_url.clone(),
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
        "logisticos.order.shipment.rescheduled",
        "logisticos.driver.delivery.failed",
        "logisticos.driver.delivery.completed",
        "logisticos.driver.delivery.attempted",
        "logisticos.driver.pickup.completed",
        "logisticos.payments.cod.collected",
        "logisticos.payments.payment.received",
        "logisticos.marketing.campaign.triggered",
    ])?;

    // HTTP server: health + rules CRUD API.
    let state = AppState {
        rule_repo:  rule_repo.clone(),
        pg_repo:    pg_repo.clone(),
        jwt:        jwt.clone(),
    };

    let app = router()
        .layer(axum::middleware::from_fn_with_state(Arc::clone(&jwt), logisticos_auth::middleware::require_auth))
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", cfg.app.host, cfg.app.port).parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(addr = %addr, "business-logic rules engine + API listening");

    let seg_checker: Arc<dyn SegmentChecker> = executor.clone();
    tokio::select! {
        result = axum::serve(listener, app) => {
            result?;
        }
        _ = run_consumer(Arc::clone(&consumer), Arc::clone(&rule_repo), executor.clone() as Arc<dyn ActionExecutor>, Arc::clone(&pg_repo), Arc::clone(&seg_checker)) => {}
        _ = run_inactive_checker(Arc::clone(&pg_repo), executor.clone() as Arc<dyn ActionExecutor>, seg_checker, cdp_url) => {}
    }

    Ok(())
}

async fn run_consumer(
    consumer: Arc<StreamConsumer>,
    rules: Arc<RuleRepository>,
    executor: Arc<dyn ActionExecutor>,
    pg_repo: Arc<PgRuleRepository>,
    seg_checker: Arc<dyn SegmentChecker>,
) {
    loop {
        match consumer.recv().await {
            Ok(msg) => {
                let tenant_id = extract_tenant_id(&msg).unwrap_or(Uuid::nil());
                let topic = msg.topic().to_owned();

                if let Some(payload) = msg.payload() {
                    if let Ok(json) = serde_json::from_slice::<Value>(payload) {
                        let matching_rules = rules.rules_for_topic(tenant_id, &topic).await;
                        let mut ctx = build_context(tenant_id, &topic, &json);
                        enrich_segments(&mut ctx, &matching_rules, seg_checker.as_ref()).await;

                        for rule in &matching_rules {
                            if rule.conditions_met(&ctx) {
                                tracing::info!(rule_id = %rule.id, rule_name = %rule.name, topic = %topic, "Rule fired");

                                let outcome = match execute_actions(rule, &ctx, executor.as_ref()).await {
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

// ---------------------------------------------------------------------------
// CustomerInactive periodic scheduler
//
// Runs every hour. Loads active rules whose trigger is `CustomerInactive`,
// queries the CDP for customers exceeding each configured inactivity threshold,
// and executes matching rule actions — identical to the event-driven path but
// time-driven rather than Kafka-driven.
// ---------------------------------------------------------------------------

async fn run_inactive_checker(
    pg_repo:     Arc<PgRuleRepository>,
    executor:    Arc<dyn ActionExecutor>,
    seg_checker: Arc<dyn SegmentChecker>,
    cdp_url:     String,
) {
    use tokio::time::{interval, Duration};
    let mut ticker = interval(Duration::from_secs(3600));
    let http = reqwest::Client::new();
    loop {
        ticker.tick().await;
        if let Err(e) = check_inactive_customers(
            pg_repo.as_ref(), executor.as_ref(), seg_checker.as_ref(), &http, &cdp_url,
        ).await {
            tracing::warn!(err = %e, "CustomerInactive checker: cycle error");
        }
    }
}

async fn check_inactive_customers(
    pg_repo:     &PgRuleRepository,
    executor:    &dyn ActionExecutor,
    seg_checker: &dyn SegmentChecker,
    http:        &reqwest::Client,
    cdp_url:     &str,
) -> anyhow::Result<()> {
    use crate::domain::entities::rule::RuleTrigger;
    use std::collections::HashMap;

    let all_rules = pg_repo.load_all().await?;

    // Build map of (tenant_id, days) → matching rules
    let mut groups: HashMap<(Uuid, u32), Vec<crate::domain::entities::rule::AutomationRule>> = HashMap::new();
    for rule in all_rules.into_iter().filter(|r| r.is_active) {
        if let RuleTrigger::CustomerInactive { days } = &rule.trigger {
            groups.entry((rule.tenant_id, *days)).or_default().push(rule);
        }
    }

    if groups.is_empty() {
        return Ok(());
    }

    let token = std::env::var("SERVICES__CDP_TOKEN").unwrap_or_default();

    for ((tenant_id, days), rules) in &groups {
        // CDP endpoint: GET /v1/customers?days_inactive=N
        // Returns `{ "data": [ { "id": "...", ... } ] }` — gracefully skip if unsupported.
        let result = http
            .get(format!("{}/v1/customers", cdp_url))
            .bearer_auth(&token)
            .header("X-Tenant-Id", tenant_id.to_string())
            .query(&[("days_inactive", days.to_string())])
            .send()
            .await;

        let customers: Vec<serde_json::Value> = match result {
            Ok(resp) if resp.status().is_success() => {
                #[derive(serde::Deserialize)]
                struct Page { profiles: Vec<serde_json::Value> }
                resp.json::<Page>().await.map(|p| p.profiles).unwrap_or_default()
            }
            Ok(resp) => {
                tracing::debug!(
                    status = %resp.status(),
                    days,
                    "CDP inactive filter not supported — skipping CustomerInactive cycle"
                );
                continue;
            }
            Err(e) => {
                tracing::warn!(err = %e, days, "CDP call failed in CustomerInactive checker");
                continue;
            }
        };

        tracing::info!(count = customers.len(), days, %tenant_id, "CustomerInactive: evaluating customers");

        for customer in customers {
            // Use external_customer_id — this matches the customer_id used in
            // order-intake/dispatch Kafka events and the CDP segment membership API.
            let Some(customer_id): Option<Uuid> =
                customer["external_customer_id"].as_str().and_then(|s| s.parse().ok())
            else { continue };

            let event_payload = serde_json::json!({
                "customer_id":   customer_id,
                "tenant_id":     tenant_id,
                "days_inactive": days,
            });
            let mut ctx = crate::application::services::build_context(
                *tenant_id,
                "logisticos.crm.customer.inactive",
                &event_payload,
            );
            crate::application::services::enrich_segments(&mut ctx, rules, seg_checker).await;

            for rule in rules {
                if rule.conditions_met(&ctx) {
                    tracing::info!(rule_id = %rule.id, %customer_id, "CustomerInactive rule fired");
                    if let Err(e) = crate::application::services::execute_actions(rule, &ctx, executor).await {
                        tracing::warn!(rule_id = %rule.id, err = %e, "CustomerInactive rule action failed");
                    }
                }
            }
        }
    }

    Ok(())
}
