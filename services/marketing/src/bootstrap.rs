use std::{net::SocketAddr, sync::Arc};
use rdkafka::{producer::FutureProducer, ClientConfig};
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

use anyhow::Context;
use logisticos_auth::jwt::JwtService;
use crate::{
    api::http,
    application::services::CampaignService,
    config::Config,
    infrastructure::{
        db::{PgAbTestRepository, PgCampaignRepository, PgJourneyRepository},
        external::CdpClient,
        messaging::KafkaEventPublisher,
    },
    AppState,
};

// ---------------------------------------------------------------------------
// Bootstrap
// ---------------------------------------------------------------------------

pub async fn run() -> anyhow::Result<()> {
    let cfg = Config::load()?;
    let otlp = std::env::var("OTLP_ENDPOINT").ok();
    logisticos_tracing::init(logisticos_tracing::TracingConfig {
        service_name: "marketing",
        env: &cfg.app.env,
        otlp_endpoint: otlp.as_deref(),
        log_level: None,
    })?;

    let pool = PgPoolOptions::new()
        .max_connections(cfg.database.max_connections)
        .after_connect(|conn, _meta| Box::pin(async move {
            sqlx::query("SET search_path TO marketing, public")
                .execute(&mut *conn)
                .await?;
            Ok(())
        }))
        .connect(&cfg.database.url)
        .await?;

    logisticos_common::migrations::run(&pool, "marketing", &sqlx::migrate!("./migrations")).await?;

    let producer: FutureProducer = ClientConfig::new()
        .set("bootstrap.servers", &cfg.kafka.brokers)
        .set("message.timeout.ms", "5000")
        .create()?;

    let campaign_repo = Arc::new(PgCampaignRepository::new(pool.clone()));
    let ab_test_repo  = Arc::new(PgAbTestRepository::new(pool.clone()));
    let journey_repo  = Arc::new(PgJourneyRepository::new(pool.clone()));
    let publisher     = Arc::new(KafkaEventPublisher::new(producer));
    let campaign_svc  = Arc::new(CampaignService::new(campaign_repo, publisher));

    // Spawn a background consumer that receives CAMPAIGN_COMPLETED events from
    // the engagement service and flips the campaign status to Completed.
    // Uses a distinct group ID suffix to avoid stealing partition assignments
    // from the main service consumer group.
    let completion_svc      = campaign_svc.clone();
    let completion_brokers  = cfg.kafka.brokers.clone();
    let completion_group_id = format!("{}-completion", cfg.kafka.group_id);
    let (completion_shutdown_tx, completion_shutdown_rx) = tokio::sync::watch::channel(false);
    tokio::spawn(async move {
        if let Err(e) = run_completion_consumer(
            completion_brokers,
            completion_group_id,
            completion_svc,
            completion_shutdown_rx,
        ).await {
            tracing::error!(err = %e, "Campaign completion consumer crashed");
        }
    });

    // Spawn a background consumer that auto-enrolls customers into journeys
    // whose trigger.type matches CAMPAIGN_OPENED/CAMPAIGN_CLICKED events.
    let enrollment_repo      = Arc::clone(&journey_repo);
    let enrollment_brokers   = cfg.kafka.brokers.clone();
    let enrollment_group_id  = format!("{}-auto-enroll", cfg.kafka.group_id);
    let (enrollment_shutdown_tx, enrollment_shutdown_rx) = tokio::sync::watch::channel(false);
    tokio::spawn(async move {
        if let Err(e) = run_auto_enrollment_consumer(
            enrollment_brokers,
            enrollment_group_id,
            enrollment_repo,
            enrollment_shutdown_rx,
        ).await {
            tracing::error!(err = %e, "Auto-enrollment consumer crashed");
        }
    });

    // Spawn a background poller that auto-activates campaigns whose scheduled_at has elapsed.
    // Runs every 60 seconds; shares the same CampaignService instance via Arc.
    let poller_svc = campaign_svc.clone();
    let (poller_shutdown_tx, poller_shutdown_rx) = tokio::sync::watch::channel(false);
    tokio::spawn(async move {
        run_scheduled_poller(poller_svc, poller_shutdown_rx).await;
    });

    let jwt_secret = std::env::var("AUTH__JWT_SECRET")
        .context("AUTH__JWT_SECRET env var not set")?;
    let jwt = Arc::new(JwtService::new(&jwt_secret, 3600, 86400));

    // Build the CDP client when both CDP_URL and CDP_TOKEN are configured.
    // Absence of either means the service runs without CDP audience resolution;
    // campaigns with explicit recipient lists still work fully.
    let cdp_client: Option<Arc<CdpClient>> = match (
        cfg.services.cdp_url.clone(),
        cfg.services.cdp_token.clone(),
    ) {
        (Some(url), Some(token)) => {
            tracing::info!(cdp_url = %url, "CDP client initialised");
            Some(Arc::new(CdpClient::new(url, token)))
        }
        _ => {
            tracing::warn!("SERVICES__CDP_URL or SERVICES__CDP_TOKEN not set — CDP audience resolution disabled");
            None
        }
    };

    // Journey execution scheduler — runs every 5 minutes, advances due enrollments.
    let journey_exec_repo = Arc::clone(&journey_repo);
    let marketing_url_for_journeys = format!("http://{}:{}", cfg.app.host, cfg.app.port);
    let journey_engagement_url = cfg.services.engagement_url.clone();
    let journey_cdp_client = cdp_client.clone();
    let (journey_shutdown_tx, journey_shutdown_rx) = tokio::sync::watch::channel(false);
    tokio::spawn(async move {
        run_journey_executor(
            journey_exec_repo, marketing_url_for_journeys,
            journey_engagement_url, journey_cdp_client,
            journey_shutdown_rx,
        ).await;
    });

    let state = AppState { campaign_svc, ab_test_repo, journey_repo, jwt: Arc::clone(&jwt), cdp_client };

    // Public routes require a valid JWT; internal routes are network-isolated.
    let app = axum::Router::new()
        .merge(
            http::router()
                .route_layer(axum::middleware::from_fn_with_state(jwt, logisticos_auth::middleware::require_auth))
        )
        .merge(http::internal_router())
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", cfg.app.host, cfg.app.port).parse()?;
    tracing::info!(addr = %addr, "marketing service listening");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    // Signal background tasks to stop after the HTTP server drains.
    completion_shutdown_tx.send(true).ok();
    enrollment_shutdown_tx.send(true).ok();
    poller_shutdown_tx.send(true).ok();
    journey_shutdown_tx.send(true).ok();

    Ok(())
}

// ---------------------------------------------------------------------------
// CAMPAIGN_COMPLETED consumer — flips campaign status to Completed
// ---------------------------------------------------------------------------

async fn run_completion_consumer(
    brokers:      String,
    group_id:     String,
    campaign_svc: Arc<CampaignService>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) -> anyhow::Result<()> {
    use rdkafka::{consumer::{CommitMode, Consumer, StreamConsumer}, ClientConfig, Message};
    use logisticos_events::topics;

    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", &brokers)
        .set("group.id", &group_id)
        .set("auto.offset.reset", "earliest")
        .set("enable.auto.commit", "false")
        .create()?;

    consumer.subscribe(&[topics::CAMPAIGN_COMPLETED])?;
    tracing::info!(group_id, "marketing completion consumer subscribed to CAMPAIGN_COMPLETED");

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow_and_update() {
                    tracing::info!("Marketing completion consumer shutting down");
                    break;
                }
            }
            result = consumer.recv() => {
                match result {
                    Ok(msg) => {
                        if let Some(bytes) = msg.payload() {
                            if let Ok(json) = serde_json::from_slice::<serde_json::Value>(bytes) {
                                handle_campaign_completed(&json, &campaign_svc).await;
                            }
                        }
                        consumer.commit_message(&msg, CommitMode::Async).ok();
                    }
                    Err(e) => {
                        tracing::error!(err = %e, "Campaign completion consumer Kafka error");
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    }
                }
            }
        }
    }
    Ok(())
}

async fn handle_campaign_completed(
    payload:      &serde_json::Value,
    campaign_svc: &CampaignService,
) {
    let Some(id_str) = payload["campaign_id"].as_str() else {
        tracing::warn!("CAMPAIGN_COMPLETED missing campaign_id — skipping");
        return;
    };
    let Ok(campaign_id) = id_str.parse::<uuid::Uuid>() else {
        tracing::warn!(campaign_id = id_str, "CAMPAIGN_COMPLETED invalid campaign_id UUID");
        return;
    };
    let total_sent      = payload["total_sent"].as_u64().unwrap_or(0);
    let total_delivered = payload["total_delivered"].as_u64().unwrap_or(0);
    let total_failed    = payload["total_failed"].as_u64().unwrap_or(0);

    match campaign_svc.complete(campaign_id, total_sent, total_delivered, total_failed).await {
        Ok(c) => tracing::info!(
            campaign_id = %campaign_id,
            total_sent,
            total_failed,
            status = ?c.status,
            "Campaign marked as completed"
        ),
        Err(e) => tracing::error!(
            campaign_id = %campaign_id,
            err = %e,
            "Failed to mark campaign as completed"
        ),
    }
}

// ---------------------------------------------------------------------------
// Auto-enrollment consumer — CAMPAIGN_OPENED / CAMPAIGN_CLICKED
//
// Enrolls a customer into every active journey in their tenant whose
// `trigger.type` matches the event (`campaign_opened` / `campaign_clicked`).
// Idempotent: skips customers already enrolled in a given journey.
// ---------------------------------------------------------------------------

async fn run_auto_enrollment_consumer(
    brokers:      String,
    group_id:     String,
    repo:         Arc<PgJourneyRepository>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) -> anyhow::Result<()> {
    use rdkafka::{consumer::{CommitMode, Consumer, StreamConsumer}, ClientConfig, Message};
    use logisticos_events::topics;

    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", &brokers)
        .set("group.id", &group_id)
        .set("auto.offset.reset", "earliest")
        .set("enable.auto.commit", "false")
        .create()?;

    consumer.subscribe(&[topics::CAMPAIGN_OPENED, topics::CAMPAIGN_CLICKED])?;
    tracing::info!(group_id, "marketing auto-enrollment consumer subscribed to CAMPAIGN_OPENED/CAMPAIGN_CLICKED");

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow_and_update() {
                    tracing::info!("Marketing auto-enrollment consumer shutting down");
                    break;
                }
            }
            result = consumer.recv() => {
                match result {
                    Ok(msg) => {
                        if let Some(bytes) = msg.payload() {
                            if let Ok(json) = serde_json::from_slice::<serde_json::Value>(bytes) {
                                handle_engagement_trigger(&json, &repo).await;
                            }
                        }
                        consumer.commit_message(&msg, CommitMode::Async).ok();
                    }
                    Err(e) => {
                        tracing::error!(err = %e, "Auto-enrollment consumer Kafka error");
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    }
                }
            }
        }
    }
    Ok(())
}

async fn handle_engagement_trigger(payload: &serde_json::Value, repo: &PgJourneyRepository) {
    let Some(trigger_type) = payload["event_type"].as_str() else { return };
    let Some(tenant_id) = payload["tenant_id"].as_str().and_then(|s| s.parse::<Uuid>().ok()) else {
        tracing::warn!("engagement trigger event missing/invalid tenant_id — skipping");
        return;
    };
    let Some(customer_id) = payload["customer_id"].as_str().and_then(|s| s.parse::<Uuid>().ok()) else {
        tracing::warn!("engagement trigger event missing/invalid customer_id — skipping");
        return;
    };

    let journey_ids = match repo.find_active_by_trigger_type(tenant_id, trigger_type).await {
        Ok(ids) => ids,
        Err(e) => {
            tracing::warn!(err = %e, trigger_type, "failed to look up journeys for trigger type");
            return;
        }
    };

    for journey_id in journey_ids {
        match repo.enrollment_exists(journey_id, customer_id).await {
            Ok(true) => continue, // already enrolled — don't reset progress
            Ok(false) => {}
            Err(e) => {
                tracing::warn!(err = %e, %journey_id, %customer_id, "enrollment_exists check failed");
                continue;
            }
        }

        let enrollment = crate::domain::entities::JourneyEnrollment {
            id: Uuid::new_v4(),
            journey_id,
            tenant_id,
            customer_id,
            current_step_order: None,
            status: "active".to_owned(),
            next_action_at: Some(chrono::Utc::now()),
            enrolled_at: chrono::Utc::now(),
        };
        match repo.save_enrollment(&enrollment).await {
            Ok(_) => tracing::info!(%journey_id, %customer_id, trigger_type, "auto-enrolled customer via trigger"),
            Err(e) => tracing::warn!(err = %e, %journey_id, %customer_id, "auto-enrollment failed"),
        }
    }
}

// ---------------------------------------------------------------------------
// Scheduled campaign poller — fires every 60 s, activates due campaigns
// ---------------------------------------------------------------------------

async fn run_scheduled_poller(
    campaign_svc: Arc<CampaignService>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    use tokio::time::{interval, MissedTickBehavior};
    let mut ticker = interval(std::time::Duration::from_secs(60));
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow_and_update() {
                    tracing::info!("Marketing scheduled poller shutting down");
                    break;
                }
            }
            _ = ticker.tick() => {
                match campaign_svc.activate_due_campaigns().await {
                    Ok(0)  => {}  // nothing due — quiet tick
                    Ok(n)  => tracing::info!(activated = n, "Scheduled poller activated due campaigns"),
                    Err(e) => tracing::error!(err = %e, "Scheduled poller error"),
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Journey execution scheduler
//
// Every 5 minutes: fetch enrollments whose next_action_at is due,
// execute the current step (send campaign or check condition), then
// advance to the next step or complete the enrollment.
// ---------------------------------------------------------------------------

async fn run_journey_executor(
    repo:           Arc<PgJourneyRepository>,
    self_url:       String,
    engagement_url: Option<String>,
    cdp_client:     Option<Arc<CdpClient>>,
    mut shutdown:   tokio::sync::watch::Receiver<bool>,
) {
    use tokio::time::{interval, Duration, MissedTickBehavior};
    let mut ticker = interval(Duration::from_secs(300)); // 5 min
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let http = reqwest::Client::new();

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow_and_update() { break; }
            }
            _ = ticker.tick() => {
                if let Err(e) = execute_due_journeys(&repo, &http, &self_url, engagement_url.as_deref(), cdp_client.as_deref()).await {
                    tracing::warn!(err = %e, "Journey executor cycle error");
                }
            }
        }
    }
    tracing::info!("Journey executor shut down");
}

/// Evaluate a journey "condition" step's yes/no branch.
///
/// `campaign_opened`/`opened` and `campaign_clicked`/`clicked` check the campaign
/// referenced by `step.condition_campaign_id`, falling back to the nearest preceding
/// `send_campaign` step in the journey when unset (the merchant-portal builder has no
/// UI to pick `condition_campaign_id` explicitly today — see apps/merchant-portal
/// crm/journeys/page.tsx). `not_opened` inverts the opened check. `clv_above_60`
/// checks the customer's CDP CLV score. Unknown condition types and calls that fail
/// (no engagement/CDP client configured, network error) default to `false` — a
/// missed "yes" branch is safer than one taken blind.
async fn evaluate_journey_condition(
    step:           &crate::domain::entities::JourneyStep,
    journey:        &crate::domain::entities::Journey,
    current_order:  i32,
    customer_id:    Uuid,
    http:           &reqwest::Client,
    engagement_url: Option<&str>,
    cdp_client:     Option<&CdpClient>,
) -> bool {
    let condition_type = step.condition_type.as_deref().unwrap_or("campaign_opened");

    match condition_type {
        "campaign_opened" | "opened" | "campaign_clicked" | "clicked" | "not_opened" => {
            let Some(campaign_id) = step.condition_campaign_id.or_else(|| {
                journey.steps.iter()
                    .filter(|s| s.step_order < current_order && s.step_type == "send_campaign")
                    .max_by_key(|s| s.step_order)
                    .and_then(|s| s.campaign_id)
            }) else {
                tracing::warn!(journey_id = %journey.id, "condition step has no campaign to check engagement against — taking no branch");
                return false;
            };
            let Some(engagement_url) = engagement_url else {
                tracing::warn!("journey condition check skipped — SERVICES__ENGAGEMENT_URL not configured");
                return false;
            };

            let url = format!("{}/v1/internal/campaign-sends/{}/{}", engagement_url, campaign_id, customer_id);
            let resp = match http.get(&url).send().await {
                Ok(r) if r.status().is_success() => r.json::<serde_json::Value>().await.ok(),
                Ok(r) => {
                    tracing::warn!(status = %r.status(), "engagement campaign-sends lookup failed");
                    None
                }
                Err(e) => {
                    tracing::warn!(err = %e, "engagement campaign-sends lookup failed");
                    None
                }
            };
            let opened  = resp.as_ref().and_then(|v| v["opened"].as_bool()).unwrap_or(false);
            let clicked = resp.as_ref().and_then(|v| v["clicked"].as_bool()).unwrap_or(false);

            match condition_type {
                "campaign_clicked" | "clicked" => clicked,
                "not_opened"                   => !opened,
                _                               => opened,
            }
        }
        "clv_above_60" => {
            let Some(cdp) = cdp_client else {
                tracing::warn!("journey CLV condition check skipped — CDP client not configured");
                return false;
            };
            match cdp.get_clv_score(journey.tenant_id, customer_id).await {
                Ok(score) => score > 60.0,
                Err(e) => {
                    tracing::warn!(err = %e, "CDP CLV lookup failed for journey condition");
                    false
                }
            }
        }
        other => {
            tracing::warn!(condition_type = other, "unknown journey condition_type — taking no branch");
            false
        }
    }
}

async fn execute_due_journeys(
    repo:           &PgJourneyRepository,
    http:           &reqwest::Client,
    self_url:       &str,
    engagement_url: Option<&str>,
    cdp_client:     Option<&CdpClient>,
) -> anyhow::Result<()> {
    use crate::domain::entities::JourneyStatus;

    let enrollments = repo.find_due_enrollments(100).await?;
    if enrollments.is_empty() { return Ok(()); }
    tracing::info!(count = enrollments.len(), "Journey executor: processing due enrollments");

    for enrollment in enrollments {
        let Some(journey) = repo.find_by_id(enrollment.journey_id).await? else {
            repo.complete_enrollment(enrollment.id).await?;
            continue;
        };
        if journey.status != JourneyStatus::Active {
            repo.complete_enrollment(enrollment.id).await?;
            continue;
        }

        let current_order = enrollment.current_step_order.unwrap_or(1);
        let Some(step) = journey.steps.iter().find(|s| s.step_order == current_order) else {
            // No step found — journey complete.
            repo.complete_enrollment(enrollment.id).await?;
            continue;
        };

        match step.step_type.as_str() {
            "send_campaign" => {
                if let Some(campaign_id) = step.campaign_id {
                    // Call our own internal trigger endpoint for this customer.
                    let url = format!("{}/v1/internal/campaigns/{}/trigger-for-recipient", self_url, campaign_id);
                    let body = serde_json::json!({
                        "customer_id": enrollment.customer_id,
                        "tenant_id":   enrollment.tenant_id,
                        "rule_id":     uuid::Uuid::nil(),
                        "rule_name":   format!("journey:{}", journey.name),
                        "shipment_id": null,
                    });
                    if let Err(e) = http.post(&url).json(&body).send().await {
                        tracing::warn!(err = %e, campaign_id = %campaign_id, "Journey send_campaign step failed");
                    }
                }
                // Advance to next sequential step immediately
                let next_order = current_order + 1;
                let next_step  = journey.steps.iter().find(|s| s.step_order == next_order);
                if next_step.is_some() {
                    repo.advance_enrollment(enrollment.id, Some(next_order), Some(chrono::Utc::now())).await?;
                } else {
                    repo.complete_enrollment(enrollment.id).await?;
                }
            }
            "wait" => {
                let days = step.wait_days.unwrap_or(1) as i64;
                let next_at = chrono::Utc::now() + chrono::Duration::days(days);
                let next_order = current_order + 1;
                let next_step  = journey.steps.iter().find(|s| s.step_order == next_order);
                if next_step.is_some() {
                    repo.advance_enrollment(enrollment.id, Some(next_order), Some(next_at)).await?;
                } else {
                    repo.complete_enrollment(enrollment.id).await?;
                }
            }
            "condition" => {
                let passed = evaluate_journey_condition(
                    step, &journey, current_order, enrollment.customer_id,
                    http, engagement_url, cdp_client,
                ).await;
                let next_order = if passed {
                    step.yes_next_order.or(Some(current_order + 1))
                } else {
                    step.no_next_order
                };
                let next_step = next_order.and_then(|order| journey.steps.iter().find(|s| s.step_order == order));
                if let (Some(next_order), Some(_)) = (next_order, next_step) {
                    repo.advance_enrollment(enrollment.id, Some(next_order), Some(chrono::Utc::now())).await?;
                } else {
                    repo.complete_enrollment(enrollment.id).await?;
                }
            }
            _ => {
                // Unknown step type — skip and advance
                let next_order = current_order + 1;
                if journey.steps.iter().any(|s| s.step_order == next_order) {
                    repo.advance_enrollment(enrollment.id, Some(next_order), Some(chrono::Utc::now())).await?;
                } else {
                    repo.complete_enrollment(enrollment.id).await?;
                }
            }
        }
    }
    Ok(())
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
    tracing::info!("marketing shutdown");
}
