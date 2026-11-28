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
    api::http::{observability_router, router, AppState},
    application::attempts::AttemptStore,
    application::services::{
        process_event,ActionExecutor, RuleRepository, SegmentChecker,
    },
    config::Config,
    domain::entities::rule::failed_delivery_rule,
    infrastructure::db::{PgAttemptStore, PgRuleRepository},
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

    let attempts: Arc<dyn AttemptStore> = Arc::new(PgAttemptStore::new(pool.clone()));
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
    let driver_ops_url = std::env::var("SERVICES__DRIVER_OPS_URL").unwrap_or_else(|_| "http://driver-ops:8006".into());
    let idle_poll_secs = parse_idle_poll_secs(std::env::var("BUSINESS_LOGIC__IDLE_POLL_SECS").ok().as_deref());
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
        "logisticos.driver.pickup.completed",
        "logisticos.payments.cod.collected",
        // PaymentReceived trigger: money actually taken (published by payments).
        "logisticos.payments.intent.captured",
        "logisticos.marketing.campaign.triggered",
        // Threshold signals: HubCapacityExceeded, PaymentOverdue, SlaBreach.
        "logisticos.hub.load.changed",
        "logisticos.payments.invoice.overdue",
        "logisticos.order.shipment.sla_breached",
    ])?;

    // HTTP server: health + rules CRUD API.
    let state = AppState {
        rule_repo:  rule_repo.clone(),
        pg_repo:    pg_repo.clone(),
        jwt:        jwt.clone(),
    };

    let app = router()
        .layer(axum::middleware::from_fn_with_state(Arc::clone(&jwt), logisticos_auth::middleware::require_auth))
        .merge(observability_router())
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
        _ = run_consumer(Arc::clone(&consumer), Arc::clone(&rule_repo), executor.clone() as Arc<dyn ActionExecutor>, Arc::clone(&pg_repo), Arc::clone(&seg_checker), attempts) => {}
        _ = run_inactive_checker(Arc::clone(&pg_repo), executor.clone() as Arc<dyn ActionExecutor>, Arc::clone(&seg_checker), cdp_url) => {}
        _ = run_idle_driver_checker(Arc::clone(&pg_repo), executor.clone() as Arc<dyn ActionExecutor>, seg_checker, driver_ops_url, idle_poll_secs) => {}
    }

    Ok(())
}

async fn run_consumer(
    consumer: Arc<StreamConsumer>,
    rules: Arc<RuleRepository>,
    executor: Arc<dyn ActionExecutor>,
    pg_repo: Arc<PgRuleRepository>,
    seg_checker: Arc<dyn SegmentChecker>,
    attempts: Arc<dyn AttemptStore>,
) {
    loop {
        match consumer.recv().await {
            Ok(msg) => {
                let header_tenant = extract_tenant_id(&msg);
                let topic = msg.topic().to_owned();

                if let Some(payload) = msg.payload() {
                    if let Ok(json) = serde_json::from_slice::<Value>(payload) {
                        match decode_event(header_tenant, &json) {
                            None => {
                                // Never evaluate rules against a nil tenant: it would
                                // match platform rules for a tenant we cannot name.
                                tracing::warn!(topic = %topic, "Event has no determinable tenant — skipped");
                            }
                            Some((tenant_id, data)) => {
                                let fired = process_event(
                                    rules.as_ref(),
                                    executor.as_ref(),
                                    seg_checker.as_ref(),
                                    attempts.as_ref(),
                                    tenant_id,
                                    &topic,
                                    event_id(&json),
                                    data,
                                ).await;
                                for f in &fired {
                                    pg_repo.log_execution(
                                        f.rule.id,
                                        f.ctx.tenant_id,
                                        &topic,
                                        f.ctx.shipment_id,
                                        true,
                                        &f.actions_logged,
                                        f.outcome,
                                        None,
                                        chrono::Utc::now(),
                                    ).await.ok();
                                }
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

/// Split a Kafka message body into `(tenant, business data)`.
///
/// Every `libs/events` publisher sends the envelope
/// `{ id, source, event_type, time, tenant_id, data }` and sets no Kafka headers;
/// a few producers send a flat object, optionally with a `tenant_id` header.
/// - Envelope form (object `data` plus uuid-string `tenant_id`): tenant is the
///   envelope's, data is `payload["data"]`.
/// - Flat form: tenant is the header, else the payload's own `tenant_id`; data is
///   the payload itself.
/// - No tenant determinable: `None`.
pub fn decode_event(headers_tenant: Option<Uuid>, payload: &Value) -> Option<(Uuid, Value)> {
    let body_tenant = payload
        .get("tenant_id")
        .and_then(Value::as_str)
        .and_then(|s| s.parse::<Uuid>().ok());

    if let (Some(tenant), Some(data)) = (body_tenant, payload.get("data").filter(|d| d.is_object())) {
        return Some((tenant, data.clone()));
    }

    headers_tenant.or(body_tenant).map(|tenant| (tenant, payload.clone()))
}

/// The envelope's event id (`Event::id`), used to dedupe redeliveries. Only
/// meaningful for the envelope form; flat payloads have none.
pub fn event_id(payload: &Value) -> Option<Uuid> {
    payload.get("data").filter(|d| d.is_object())?;
    payload.get("id").and_then(Value::as_str).and_then(|s| s.parse().ok())
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

// ---------------------------------------------------------------------------
// DriverIdleExceeded periodic poller
//
// Idle is the absence of events, so it is polled. Each tick asks driver-ops which
// drivers crossed each rule's threshold since the previous tick (the crossing
// window equals the poll interval), then runs the rule exactly like the Kafka
// path, including the execution log.
// ---------------------------------------------------------------------------

const IDLE_TOPIC: &str = "logisticos.driver.idle_exceeded";
const DEFAULT_IDLE_POLL_SECS: u64 = 60;

/// `BUSINESS_LOGIC__IDLE_POLL_SECS`: default 60; 0 or unparsable falls back to 60.
/// The value is also driver-ops' `window_secs` (accepts 30..=3600), so clamp it:
/// a configured value must never turn every poll into a 400.
fn parse_idle_poll_secs(raw: Option<&str>) -> u64 {
    match raw {
        None => DEFAULT_IDLE_POLL_SECS,
        Some(v) => match v.trim().parse::<u64>() {
            Ok(n) if n > 0 => n.clamp(30, 3600),
            _ => {
                tracing::warn!(value = v, "invalid BUSINESS_LOGIC__IDLE_POLL_SECS; using 60");
                DEFAULT_IDLE_POLL_SECS
            }
        },
    }
}

/// Longest span driver-ops accepts for `since`, less a safety margin so that
/// clock skew and request latency never push a clamped watermark over its limit.
const MAX_SINCE_AGE_SECS: i64 = 3600 - 60;

type Watermarks = std::collections::HashMap<(Uuid, u32), chrono::DateTime<chrono::Utc>>;

/// Poller state that lives across ticks.
#[derive(Default)]
struct IdlePollState {
    warned_global: bool,
    watermarks:    Watermarks,
}

/// How to tell driver-ops where the next poll starts.
#[derive(Debug, PartialEq)]
enum PollFrom {
    /// No watermark yet (first poll after start): cover one poll interval back.
    Window(u64),
    /// Resume from the previous response as_of.
    Since(chrono::DateTime<chrono::Utc>),
}

/// Choose the query for a group. A watermark older than the accepted span is
/// clamped; the second value is the span of episodes that can no longer be covered.
fn poll_from(
    watermark: Option<chrono::DateTime<chrono::Utc>>,
    now: chrono::DateTime<chrono::Utc>,
    poll_secs: u64,
) -> (PollFrom, Option<chrono::Duration>) {
    let Some(w) = watermark else { return (PollFrom::Window(poll_secs), None) };
    let oldest = now - chrono::Duration::seconds(MAX_SINCE_AGE_SECS);
    if w < oldest { (PollFrom::Since(oldest), Some(oldest - w)) } else { (PollFrom::Since(w), None) }
}

/// Drop watermarks for groups that no longer have an active rule.
fn prune_watermarks(wm: &mut Watermarks, groups: &IdleGroups) {
    wm.retain(|k, _| groups.contains_key(k));
}

/// Advance a group watermark after a successful driver-ops response. On a
/// driver-ops error this is simply not called, so the next tick re-covers the gap.
fn advance_watermark(wm: &mut Watermarks, key: (Uuid, u32), as_of: chrono::DateTime<chrono::Utc>) {
    wm.insert(key, as_of);
}

type IdleGroups = std::collections::HashMap<(Uuid, u32), Vec<crate::domain::entities::rule::AutomationRule>>;

/// Group active `DriverIdleExceeded` rules by (tenant, minutes). Returns the
/// groups and how many global (nil-tenant) rules were skipped.
fn group_idle_rules(rules: Vec<crate::domain::entities::rule::AutomationRule>) -> (IdleGroups, usize) {
    use crate::domain::entities::rule::RuleTrigger;
    let mut groups = IdleGroups::new();
    let mut skipped = 0;
    for rule in rules.into_iter().filter(|r| r.is_active) {
        if let RuleTrigger::DriverIdleExceeded { minutes } = &rule.trigger {
            if rule.tenant_id == Uuid::nil() {
                skipped += 1;
                continue;
            }
            groups.entry((rule.tenant_id, *minutes)).or_default().push(rule);
        }
    }
    (groups, skipped)
}

async fn run_idle_driver_checker(
    pg_repo:        Arc<PgRuleRepository>,
    executor:       Arc<dyn ActionExecutor>,
    seg_checker:    Arc<dyn SegmentChecker>,
    driver_ops_url: String,
    poll_secs:      u64,
) {
    use tokio::time::{interval, Duration};
    let mut ticker = interval(Duration::from_secs(poll_secs));
    // After a stall, run one poll late and carry on; Burst would fire a catch-up
    // flurry of back-to-back polls.
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap_or_default();
    // In memory only: a business-logic restart loses it, so episodes that cross
    // the threshold while this service is down can be missed (the first poll
    // after start covers only one poll interval back).
    let mut state = IdlePollState::default();
    loop {
        ticker.tick().await;
        if let Err(e) = check_idle_drivers(
            pg_repo.as_ref(), executor.as_ref(), seg_checker.as_ref(), &http, &driver_ops_url,
            poll_secs, &mut state,
        ).await {
            tracing::warn!(err = %e, "DriverIdleExceeded checker: cycle error");
        }
    }
}

async fn check_idle_drivers(
    pg_repo:        &PgRuleRepository,
    executor:       &dyn ActionExecutor,
    seg_checker:    &dyn SegmentChecker,
    http:           &reqwest::Client,
    driver_ops_url: &str,
    poll_secs:      u64,
    state:          &mut IdlePollState,
) -> anyhow::Result<()> {
    let (groups, skipped) = group_idle_rules(pg_repo.load_all().await?);
    if skipped > 0 && !state.warned_global {
        state.warned_global = true;
        tracing::info!(skipped, "global (nil-tenant) DriverIdleExceeded rules are not supported; skipping");
    }

    prune_watermarks(&mut state.watermarks, &groups);

    for ((tenant_id, minutes), rules) in &groups {
        let key = (*tenant_id, *minutes);
        let (from, dropped) = poll_from(state.watermarks.get(&key).copied(), chrono::Utc::now(), poll_secs);
        if let Some(span) = dropped {
            tracing::warn!(%tenant_id, minutes, dropped_secs = span.num_seconds(),
                "idle watermark older than the accepted span; episodes in the dropped span are lost");
        }
        let mut query = vec![
            ("tenant_id", tenant_id.to_string()),
            ("idle_minutes", minutes.to_string()),
        ];
        match from {
            PollFrom::Window(secs) => query.push(("window_secs", secs.to_string())),
            PollFrom::Since(s) => query.push((
                "since",
                s.to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
            )),
        }
        let resp = http
            .get(format!("{}/v1/internal/drivers/idle", driver_ops_url))
            .query(&query)
            .send()
            .await;
        let (drivers, as_of): (Vec<Value>, chrono::DateTime<chrono::Utc>) = match resp {
            Ok(r) if r.status().is_success() => match r.json::<Value>().await {
                Ok(v) => {
                    let Some(as_of) = v["as_of"].as_str().and_then(|s| s.parse().ok()) else {
                        tracing::warn!(minutes, %tenant_id, "driver-ops idle response has no as_of");
                        continue;
                    };
                    (v["data"].as_array().cloned().unwrap_or_default(), as_of)
                }
                Err(e) => {
                    tracing::warn!(err = %e, minutes, %tenant_id, "driver-ops idle response unreadable");
                    continue;
                }
            },
            Ok(r) => {
                tracing::warn!(status = %r.status(), minutes, %tenant_id, "driver-ops idle query refused");
                continue;
            }
            Err(e) => {
                tracing::warn!(err = %e, minutes, %tenant_id, "driver-ops idle call failed");
                continue;
            }
        };

        // Advance on any successful response, even if actions below fail: those
        // outcomes are recorded by log_execution and must not re-fire.
        advance_watermark(&mut state.watermarks, key, as_of);

        for d in drivers {
            let payload = serde_json::json!({
                "driver_id":    d["driver_id"],
                "tenant_id":    tenant_id,
                "idle_since":   d["idle_since"],
                "idle_minutes": d["idle_minutes"],
            });
            let mut ctx = crate::application::services::build_context(*tenant_id, IDLE_TOPIC, &payload);
            crate::application::services::enrich_segments(&mut ctx, rules, seg_checker).await;

            for rule in rules {
                if !rule.conditions_met(&ctx) { continue; }
                tracing::info!(rule_id = %rule.id, driver_id = %d["driver_id"], "DriverIdleExceeded rule fired");
                let (outcome, actions_log) = match crate::application::services::execute_actions(rule, &ctx, executor).await {
                    Ok(report) => (report.outcome(), report.logged_actions()),
                    Err(e) => {
                        tracing::error!(rule_id = %rule.id, err = %e, "Rule action failed");
                        ("error", rule.actions.iter().map(|a| format!("{:?}", a)).collect::<Vec<_>>())
                    }
                };
                if let Err(e) = pg_repo.log_execution(
                    rule.id,
                    ctx.tenant_id,
                    IDLE_TOPIC,
                    ctx.shipment_id,
                    true,
                    &actions_log,
                    outcome,
                    None,
                    chrono::Utc::now(),
                ).await {
                    tracing::warn!(rule_id = %rule.id, err = %e, "failed to log rule execution");
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::decode_event;
    use super::{
        advance_watermark, group_idle_rules, parse_idle_poll_secs, poll_from, prune_watermarks,
        PollFrom, Watermarks, MAX_SINCE_AGE_SECS,
    };
    use chrono::{Duration, TimeZone, Utc};
    use crate::domain::entities::rule::{AutomationRule, RuleTrigger};

    fn rule(tenant: Uuid, trigger: RuleTrigger, active: bool) -> AutomationRule {
        AutomationRule {
            id: Uuid::new_v4(), tenant_id: tenant, name: "r".into(), description: String::new(),
            is_active: active, trigger, conditions: vec![], actions: vec![], priority: 1,
            created_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn idle_rules_group_by_tenant_and_minutes() {
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        let idle = |m| RuleTrigger::DriverIdleExceeded { minutes: m };
        let (g, skipped) = group_idle_rules(vec![
            rule(a, idle(30), true), rule(a, idle(30), true), rule(a, idle(45), true),
            rule(b, idle(30), true),
            rule(a, idle(30), false),
            rule(a, RuleTrigger::DeliveryFailed, true),
        ]);
        assert_eq!(skipped, 0);
        assert_eq!(g.len(), 3);
        assert_eq!(g[&(a, 30)].len(), 2);
        assert_eq!(g[&(a, 45)].len(), 1);
        assert_eq!(g[&(b, 30)].len(), 1);
    }

    #[test]
    fn nil_tenant_idle_rules_are_skipped_and_counted() {
        let t = Uuid::new_v4();
        let idle = RuleTrigger::DriverIdleExceeded { minutes: 30 };
        let (g, skipped) = group_idle_rules(vec![
            rule(Uuid::nil(), idle.clone(), true), rule(Uuid::nil(), idle.clone(), true), rule(t, idle, true),
        ]);
        assert_eq!(skipped, 2);
        assert_eq!(g.len(), 1);
        assert!(g.contains_key(&(t, 30)));
    }

    #[test]
    fn idle_poll_secs_defaults_and_rejects_bad_values() {
        assert_eq!(parse_idle_poll_secs(None), 60);
        assert_eq!(parse_idle_poll_secs(Some("0")), 60);
        assert_eq!(parse_idle_poll_secs(Some("abc")), 60);
        assert_eq!(parse_idle_poll_secs(Some("120")), 120);
        assert_eq!(parse_idle_poll_secs(Some("10")), 30);
        assert_eq!(parse_idle_poll_secs(Some("99999")), 3600);
    }

    use serde_json::json;
    use uuid::Uuid;

    #[test]
    fn envelope_form_uses_envelope_tenant_and_data() {
        let tenant = Uuid::new_v4();
        let shipment = Uuid::new_v4();
        let payload = json!({
            "id": Uuid::new_v4(), "source": "driver-ops", "event_type": "delivery.failed",
            "time": "2026-11-25T00:00:00Z", "tenant_id": tenant,
            "data": {"shipment_id": shipment}
        });
        // The header, if any, never overrides the envelope.
        let (t, data) = decode_event(Some(Uuid::new_v4()), &payload).expect("decodes");
        assert_eq!(t, tenant);
        assert_eq!(data, json!({"shipment_id": shipment}));
    }

    #[test]
    fn flat_form_uses_header_tenant() {
        let tenant = Uuid::new_v4();
        let payload = json!({"shipment_id": "x"});
        let (t, data) = decode_event(Some(tenant), &payload).expect("decodes");
        assert_eq!(t, tenant);
        assert_eq!(data, payload);
    }

    #[test]
    fn flat_form_falls_back_to_body_tenant() {
        let tenant = Uuid::new_v4();
        let payload = json!({"tenant_id": tenant, "campaign_id": "c"});
        let (t, data) = decode_event(None, &payload).expect("decodes");
        assert_eq!(t, tenant);
        assert_eq!(data, payload);
    }

    #[test]
    fn no_tenant_is_none() {
        assert!(decode_event(None, &json!({"shipment_id": "x"})).is_none());
        assert!(decode_event(None, &json!({"tenant_id": "not-a-uuid", "data": {}})).is_none());
    }

    #[test]
    fn first_poll_uses_window_secs() {
        let now = Utc.with_ymd_and_hms(2026, 11, 25, 12, 0, 0).unwrap();
        assert_eq!(poll_from(None, now, 60), (PollFrom::Window(60), None));
    }

    #[test]
    fn watermark_is_sent_as_since_and_advances_on_success() {
        let now = Utc.with_ymd_and_hms(2026, 11, 25, 12, 0, 0).unwrap();
        let key = (Uuid::new_v4(), 30);
        let mut wm = Watermarks::new();
        let as_of_1 = now - Duration::seconds(300);
        advance_watermark(&mut wm, key, as_of_1);
        assert_eq!(poll_from(wm.get(&key).copied(), now, 60), (PollFrom::Since(as_of_1), None));
        advance_watermark(&mut wm, key, now);
        assert_eq!(wm[&key], now);
    }

    #[test]
    fn watermark_is_retained_when_driver_ops_errors() {
        // The error path never calls advance_watermark; the next tick re-sends the same since.
        let now = Utc.with_ymd_and_hms(2026, 11, 25, 12, 0, 0).unwrap();
        let key = (Uuid::new_v4(), 30);
        let mut wm = Watermarks::new();
        let w = now - Duration::seconds(120);
        advance_watermark(&mut wm, key, w);
        let later = now + Duration::seconds(60);
        assert_eq!(poll_from(wm.get(&key).copied(), later, 60).0, PollFrom::Since(w));
    }

    #[test]
    fn stale_watermark_is_clamped_and_reports_dropped_span() {
        let now = Utc.with_ymd_and_hms(2026, 11, 25, 12, 0, 0).unwrap();
        let w = now - Duration::seconds(7200);
        let (from, dropped) = poll_from(Some(w), now, 60);
        let oldest = now - Duration::seconds(MAX_SINCE_AGE_SECS);
        assert_eq!(from, PollFrom::Since(oldest));
        assert_eq!(dropped, Some(oldest - w));
    }

    #[test]
    fn watermarks_for_vanished_groups_are_dropped() {
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        let idle = |m| RuleTrigger::DriverIdleExceeded { minutes: m };
        let (groups, _) = group_idle_rules(vec![rule(a, idle(30), true)]);
        let mut wm = Watermarks::new();
        let t = Utc.with_ymd_and_hms(2026, 11, 25, 12, 0, 0).unwrap();
        advance_watermark(&mut wm, (a, 30), t);
        advance_watermark(&mut wm, (b, 30), t);
        advance_watermark(&mut wm, (a, 45), t);
        prune_watermarks(&mut wm, &groups);
        assert_eq!(wm.keys().copied().collect::<Vec<_>>(), vec![(a, 30)]);
    }
}
