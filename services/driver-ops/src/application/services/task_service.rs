use std::sync::Arc;
use logisticos_errors::{AppError, AppResult};
use logisticos_types::{DriverId, TenantId};
use logisticos_events::{producer::KafkaProducer, topics, envelope::Event};
use uuid::Uuid;

use crate::{
    application::commands::{StartTaskCommand, CompleteTaskCommand, FailTaskCommand, LeaveTaskCommand, TaskSummary},
    domain::{
        entities::{DriverTask, TaskStatus, TaskType},
        events::{TaskCompleted, TaskFailed},
        repositories::{
            DailyEarning, DriverRepository, EarningAdjustment, EarningEntry, JobDrop, JobDropRepository,
            TaskRepository, TenantTaskSummary,
        },
        value_objects::leave_policy::{
            acceptance_pct, drop_fee_cents, leave_mode, minutes_past, waiting_fee_cents,
            LeaveMode, LeaveQuote, PenaltyPolicy, StopState,
        },
    },
};

/// The reason a released stop fails with — the code the app's own failed-stop
/// sheet already sends for "customer not home".
pub const RELEASE_REASON: &str = "CUSTOMER_ABSENT";

/// Earnings view returned by `GET /v1/drivers/me/earnings`.
/// `today_cents` / `week_cents` are derived from `daily` server-side so the
/// app renders the summary card without re-aggregating.
#[derive(Debug, serde::Serialize)]
pub struct EarningsView {
    pub today_cents: i64,
    pub week_cents:  i64,
    pub daily:       Vec<DailyEarning>,
    pub entries:     Vec<EarningEntry>,
    /// Waiting pay and drop fees. Already netted into the totals above.
    pub adjustments: Vec<EarningAdjustment>,
}

/// The driver's job, as seen from the stop they asked to leave.
struct JobView {
    stop: DriverTask,
    /// The driver's open tasks for this shipment on this route, the stop included.
    open_tasks: Vec<DriverTask>,
    goods_aboard: bool,
    /// The contractual payout the drop fee is a share of. 0 for full-time drivers.
    payout_cents: i64,
}

pub struct TaskService {
    task_repo: Arc<dyn TaskRepository>,
    driver_repo: Arc<dyn DriverRepository>,
    drop_repo: Arc<dyn JobDropRepository>,
    kafka: Arc<KafkaProducer>,
    policy: PenaltyPolicy,
}

impl TaskService {
    pub fn new(
        task_repo: Arc<dyn TaskRepository>,
        driver_repo: Arc<dyn DriverRepository>,
        drop_repo: Arc<dyn JobDropRepository>,
        kafka: Arc<KafkaProducer>,
        policy: PenaltyPolicy,
    ) -> Self {
        Self { task_repo, driver_repo, drop_repo, kafka, policy: policy.clamped() }
    }

    /// Returns the driver's current task queue — pending and in-progress tasks for their active route.
    /// Aggregated manifest for the partner portal. Delegates to the repo
    /// which runs a single SQL group-by; the service layer exists so the
    /// HTTP handler never touches the pool directly.
    pub async fn tenant_summary(&self, tenant_id: &TenantId) -> AppResult<TenantTaskSummary> {
        let today = chrono::Utc::now().date_naive();
        self.task_repo
            .tenant_summary(tenant_id, today)
            .await
            .map_err(AppError::Internal)
    }

    pub async fn list_manifest(
        &self,
        tenant_id: &logisticos_types::TenantId,
        carrier_id: Option<uuid::Uuid>,
        date: chrono::NaiveDate,
    ) -> AppResult<Vec<crate::domain::repositories::ManifestEntry>> {
        self.task_repo
            .list_manifest(tenant_id, carrier_id, date)
            .await
            .map_err(AppError::Internal)
    }

    /// Earnings history + daily aggregates for the authenticated driver.
    /// `from`/`to` window the entry list; the daily series always covers at
    /// least the trailing 7 days so the summary card and sparkline are stable
    /// regardless of the requested page window.
    pub async fn my_earnings(
        &self,
        driver_id: &DriverId,
        from: chrono::DateTime<chrono::Utc>,
        to: chrono::DateTime<chrono::Utc>,
        limit: i64,
        offset: i64,
    ) -> AppResult<EarningsView> {
        let now = chrono::Utc::now();
        let today_start = now.date_naive().and_hms_opt(0, 0, 0)
            .unwrap_or_default().and_utc();
        let week_start = today_start - chrono::Duration::days(6);

        let daily_from = week_start.min(from);
        let daily = self.task_repo
            .daily_earnings(driver_id.inner(), daily_from, now.max(to))
            .await.map_err(AppError::Internal)?;

        let today = now.date_naive();
        let today_cents = daily.iter()
            .find(|d| d.day == today)
            .map(|d| d.total_cents).unwrap_or(0);
        let week_cents = daily.iter()
            .filter(|d| d.day >= week_start.date_naive() && d.day <= today)
            .map(|d| d.total_cents).sum();

        let entries = self.task_repo
            .list_earnings(driver_id.inner(), from, to, limit, offset)
            .await.map_err(AppError::Internal)?;

        let adjustments = self.drop_repo
            .list_adjustments(driver_id.inner(), from, to)
            .await.map_err(AppError::Internal)?;

        Ok(EarningsView { today_cents, week_cents, daily, entries, adjustments })
    }

    pub async fn list_my_tasks(&self, driver_id: &DriverId) -> AppResult<Vec<TaskSummary>> {
        let payout = self.payout_for_driver(driver_id).await;
        let tasks = self.task_repo.list_by_driver(driver_id).await.map_err(AppError::Internal)?;
        Ok(tasks.into_iter()
            .filter(|t| matches!(t.status, TaskStatus::Pending | TaskStatus::InProgress))
            .map(|t| Self::to_summary(t, payout))
            .collect()
        )
    }

    /// Returns ALL tasks for a driver regardless of status (for history view).
    /// Unlike `list_my_tasks`, completed/failed/cancelled tasks are included.
    pub async fn list_all_tasks(&self, driver_id: &DriverId) -> AppResult<Vec<TaskSummary>> {
        let payout = self.payout_for_driver(driver_id).await;
        let tasks = self.task_repo.list_by_driver(driver_id).await.map_err(AppError::Internal)?;
        Ok(tasks.into_iter()
            .map(|t| Self::to_summary(t, payout))
            .collect()
        )
    }

    /// Per-task payout shown on the driver's task card. ONLY part-time (gig)
    /// drivers see a price — full-time drivers are salaried, so `None` here
    /// keeps the payment chip hidden in the app. Best-effort: a driver-row
    /// lookup failure degrades to "no payout shown", never an error.
    async fn payout_for_driver(&self, driver_id: &DriverId) -> Option<i64> {
        use crate::domain::entities::DriverType;
        match self.driver_repo.find_by_user_id(driver_id.inner()).await {
            Ok(Some(d)) if d.driver_type == DriverType::PartTime =>
                Some(i64::from(d.per_delivery_rate_cents)),
            _ => None,
        }
    }

    fn to_summary(t: DriverTask, payout_cents: Option<i64>) -> TaskSummary {
        let is_delivery = matches!(t.task_type, TaskType::Delivery);
        TaskSummary {
            task_id:           t.id,
            shipment_id:       t.shipment_id,
            sequence:          t.sequence as u32,
            status:            format!("{:?}", t.status).to_lowercase(),
            task_type:         format!("{:?}", t.task_type).to_lowercase(),
            customer_name:     t.customer_name.clone(),
            customer_phone:    t.customer_phone.clone(),
            address:           format!("{}, {}", t.address.line1, t.address.city),
            tracking_number:   t.tracking_number.clone(),
            cod_amount_cents:  t.cod_amount_cents,
            lat:               t.address.coordinates.map(|c| c.lat),
            lng:               t.address.coordinates.map(|c| c.lng),
            merchant_name:     t.merchant_name.clone(),
            delivery_category: t.delivery_category.clone(),
            weight_grams:      t.weight_grams,
            pickup_lat:        t.pickup_lat,
            pickup_lng:        t.pickup_lng,
            delivery_lat:      t.delivery_lat,
            delivery_lng:      t.delivery_lng,
            // Contractual snapshot wins; the live per-delivery rate is only a
            // display fallback for rows created before the snapshot column.
            payout_cents:      t.payout_cents.or(payout_cents),
            // Pickup: AWB + parcel photo. Delivery: AWB + parcel photo +
            // signature + the recipient's delivery PIN. The PIN used to be
            // asked for only on COD drops; pod now refuses every POD without it
            // (DELIVERY_PIN__REQUIRED), so asking for less here would strand
            // the driver at a submit that cannot succeed.
            requires_photo:     true,
            requires_signature: is_delivery,
            requires_otp:       is_delivery,
            started_at:         t.started_at,
            grace_expires_at:   t.grace_expires_at,
        }
    }

    pub async fn start_task(&self, driver_id: &DriverId, cmd: StartTaskCommand) -> AppResult<()> {
        let mut task = self.fetch_and_validate_ownership(driver_id, cmd.task_id).await?;

        if task.status != TaskStatus::Pending {
            return Err(AppError::BusinessRule("Can only start a pending task".into()));
        }

        // Arrival starts the grace clock, from the policy as it stands now.
        let now = chrono::Utc::now();
        task.start_at(now, self.policy.grace_deadline(now));
        self.task_repo.save(&task).await.map_err(AppError::Internal)?;
        tracing::info!(task_id = %task.id, driver_id = %driver_id, "Task started");
        Ok(())
    }

    pub async fn complete_task(
        &self,
        driver_id: &DriverId,
        tenant_id: &TenantId,
        cmd: CompleteTaskCommand,
    ) -> AppResult<()> {
        let mut task = self.fetch_and_validate_ownership(driver_id, cmd.task_id).await?;

        if task.status != TaskStatus::InProgress {
            return Err(AppError::BusinessRule("Can only complete an in-progress task".into()));
        }

        // Both task types require evidence before completion.
        if task.task_type == TaskType::Delivery && cmd.pod_id.is_none() {
            return Err(AppError::BusinessRule(
                "delivery task completion requires pod_id".into(),
            ));
        }
        if task.task_type == TaskType::Pickup && cmd.pop_id.is_none() {
            return Err(AppError::BusinessRule(
                "pickup task completion requires pop_id".into(),
            ));
        }

        task.complete(cmd.pod_id, cmd.pop_id);

        self.task_repo.save(&task).await.map_err(AppError::Internal)?;

        // Determine event topic based on task type — pickup vs delivery
        let (event_type, topic) = match task.task_type {
            TaskType::Pickup   => ("pickup.completed",   topics::PICKUP_COMPLETED),
            TaskType::Delivery => ("delivery.completed",  topics::DELIVERY_COMPLETED),
        };

        // Publish event — engagement sends receipt/notification to customer,
        // payments processes COD reconciliation if applicable.
        let event = Event::new("driver-ops", event_type, tenant_id.inner(), TaskCompleted {
            task_id: task.id,
            driver_id: driver_id.inner(),
            shipment_id: task.shipment_id,
            tenant_id: tenant_id.inner(),
            pod_id: cmd.pod_id,
            pop_id: cmd.pop_id,
            completed_at: task.completed_at.unwrap_or_else(chrono::Utc::now),
            customer_name: task.customer_name.clone(),
            customer_phone: task.customer_phone.clone(),
            customer_email: task.customer_email.clone().unwrap_or_default(),
            tracking_number: task.tracking_number.clone().unwrap_or_default(),
            cod_amount_cents: task.cod_amount_cents,
            // customer_id not yet stored on DriverTask (requires TaskAssigned to carry it).
            // Engagement service falls back to shipment_id as the audit key when absent.
            customer_id: task.customer_id,
        });
        self.kafka.publish_event(topic, &event).await
            .map_err(AppError::Internal)?;

        tracing::info!(task_id = %task.id, driver_id = %driver_id, pod_id = ?cmd.pod_id, "Task completed");
        Ok(())
    }

    pub async fn fail_task(
        &self,
        driver_id: &DriverId,
        tenant_id: &TenantId,
        cmd: FailTaskCommand,
    ) -> AppResult<()> {
        let mut task = self.fetch_and_validate_ownership(driver_id, cmd.task_id).await?;

        if !matches!(task.status, TaskStatus::InProgress | TaskStatus::Pending) {
            return Err(AppError::BusinessRule("Can only fail an active task".into()));
        }

        task.fail(cmd.reason.clone());
        self.task_repo.save(&task).await.map_err(AppError::Internal)?;

        // Publish event — business-logic service will apply ECA rules (retry, re-assign, notify)
        let event = Event::new("driver-ops", "delivery.failed", tenant_id.inner(), TaskFailed {
            task_id: task.id,
            driver_id: driver_id.inner(),
            shipment_id: task.shipment_id,
            tenant_id: tenant_id.inner(),
            reason: cmd.reason,
            failed_at: task.completed_at.unwrap_or_else(chrono::Utc::now),
            customer_name: task.customer_name.clone(),
            customer_phone: task.customer_phone.clone(),
            tracking_number: task.tracking_number.clone().unwrap_or_default(),
            customer_id: task.customer_id,
            waiting_fee_cents: None,
        });
        self.kafka.publish_event(topics::DELIVERY_FAILED, &event).await
            .map_err(AppError::Internal)?;

        tracing::info!(task_id = %task.id, driver_id = %driver_id, "Task failed");
        Ok(())
    }

    /// The number the masked-call bridge rings on the driver's side. Only the
    /// driver currently on the job: once they finish, fail or drop it, there
    /// is no one to put the customer through to.
    pub async fn driver_phone_on_shipment(&self, shipment_id: Uuid) -> AppResult<Option<String>> {
        self.task_repo.driver_phone_on_shipment(shipment_id).await.map_err(AppError::Internal)
    }

    /// Acceptance over impression-verified offers, net of drops. None before
    /// any offer has been seen, or where dispatch's offers are not reachable.
    pub async fn acceptance(&self, user_id: Uuid) -> Option<i64> {
        let (seen, claimed) = self.driver_repo.get_offer_stats(user_id).await.ok().flatten()?;
        let drops = self.drop_repo.offer_drop_count(user_id).await.unwrap_or(0);
        acceptance_pct(seen, claimed, drops, self.policy.drop_counts_as_declines)
    }

    /// What leaving this job would cost, before the driver commits.
    pub async fn leave_quote(&self, driver_id: &DriverId, task_id: Uuid) -> AppResult<LeaveQuote> {
        let job = self.job_view(driver_id, task_id).await?;
        let now = chrono::Utc::now();
        let mode = self.mode_for(&job, now)?;
        Ok(self.price(&job, mode, now).await)
    }

    /// Leave the job. The server decides drop or release; the answer is the
    /// quote as applied.
    pub async fn leave(
        &self,
        driver_id: &DriverId,
        tenant_id: &TenantId,
        task_id: Uuid,
        cmd: LeaveTaskCommand,
    ) -> AppResult<LeaveQuote> {
        let job = self.job_view(driver_id, task_id).await?;

        // A retry after a drop that committed but whose event did not go out:
        // replay the event, charge nothing new.
        if job.stop.status == TaskStatus::Cancelled {
            if let Some(prior) = self.drop_repo
                .find(job.stop.driver_id.inner(), job.stop.shipment_id, job.stop.route_id)
                .await.map_err(AppError::Internal)?
            {
                self.publish_drop(&prior).await?;
                return Ok(LeaveQuote {
                    mode: LeaveMode::Drop,
                    fee_cents: prior.fee_cents,
                    payout_cents: prior.payout_cents,
                    fee_pct: self.policy.drop_fee_pct,
                    waiting_fee_cents: 0,
                    waiting_fee_cents_per_hour: self.policy.waiting_fee_cents_per_hour,
                    minutes_past_grace: 0,
                    grace_expires_at: job.stop.grace_expires_at,
                    acceptance_before: None,
                    acceptance_after: None,
                });
            }
        }

        let now = chrono::Utc::now();
        let mode = self.mode_for(&job, now)?;
        let quote = self.price(&job, mode, now).await;

        match mode {
            LeaveMode::Drop => {
                let drop = JobDrop {
                    tenant_id:       tenant_id.inner(),
                    driver_id:       job.stop.driver_id.inner(),
                    route_id:        job.stop.route_id,
                    shipment_id:     job.stop.shipment_id,
                    tracking_number: job.stop.tracking_number.clone(),
                    reason_code:     cmd.reason_code,
                    note:            cmd.note.filter(|n| !n.trim().is_empty()),
                    lat:             cmd.lat,
                    lng:             cmd.lng,
                    payout_cents:    job.payout_cents,
                    fee_cents:       quote.fee_cents,
                    dropped_at:      now,
                };
                let task_ids: Vec<Uuid> = job.open_tasks.iter().map(|t| t.id).collect();
                if !self.drop_repo.drop_job(&drop, &task_ids).await.map_err(AppError::Internal)? {
                    return Err(AppError::Conflict("TASK_CLOSED".into()));
                }
                self.publish_drop(&drop).await?;
                tracing::info!(
                    task_id = %task_id, driver_id = %driver_id, shipment_id = %drop.shipment_id,
                    fee_cents = drop.fee_cents, "Job dropped"
                );
            }
            LeaveMode::Release => {
                let mut stop = job.stop.clone();
                stop.release(RELEASE_REASON.into(), quote.waiting_fee_cents);
                self.task_repo.save(&stop).await.map_err(AppError::Internal)?;
                // Nobody handed over the load, so there is nothing to deliver.
                if stop.task_type == TaskType::Pickup {
                    for mut rest in job.open_tasks.into_iter().filter(|t| t.id != stop.id) {
                        rest.status = TaskStatus::Cancelled;
                        self.task_repo.save(&rest).await.map_err(AppError::Internal)?;
                    }
                }
                let event = Event::new("driver-ops", "delivery.failed", tenant_id.inner(), TaskFailed {
                    task_id: stop.id,
                    driver_id: driver_id.inner(),
                    shipment_id: stop.shipment_id,
                    tenant_id: tenant_id.inner(),
                    reason: RELEASE_REASON.into(),
                    failed_at: stop.completed_at.unwrap_or(now),
                    customer_name: stop.customer_name.clone(),
                    customer_phone: stop.customer_phone.clone(),
                    tracking_number: stop.tracking_number.clone().unwrap_or_default(),
                    customer_id: stop.customer_id,
                    waiting_fee_cents: Some(quote.waiting_fee_cents),
                });
                self.kafka.publish_event(topics::DELIVERY_FAILED, &event).await
                    .map_err(AppError::Internal)?;
                tracing::info!(
                    task_id = %task_id, driver_id = %driver_id,
                    waiting_fee_cents = quote.waiting_fee_cents, "Stop released after grace"
                );
            }
        }
        Ok(quote)
    }

    async fn job_view(&self, driver_id: &DriverId, task_id: Uuid) -> AppResult<JobView> {
        let stop = self.fetch_and_validate_ownership(driver_id, task_id).await?;
        let same_job: Vec<DriverTask> = self.task_repo
            .list_by_route(stop.route_id).await.map_err(AppError::Internal)?
            .into_iter()
            .filter(|t| t.shipment_id == stop.shipment_id && t.driver_id == stop.driver_id)
            .collect();
        // No pickup leg on this driver's route means the load came from the
        // hub: it is aboard from the start.
        let goods_aboard = same_job.iter()
            .find(|t| t.task_type == TaskType::Pickup)
            .is_none_or(|pickup| pickup.status == TaskStatus::Completed);
        let payout_cents = same_job.iter().filter_map(|t| t.payout_cents).max().unwrap_or(0);
        let open_tasks = same_job.into_iter().filter(DriverTask::is_open).collect();
        Ok(JobView { stop, open_tasks, goods_aboard, payout_cents })
    }

    fn mode_for(&self, job: &JobView, now: chrono::DateTime<chrono::Utc>) -> AppResult<LeaveMode> {
        let state = StopState {
            open: job.stop.is_open(),
            grace_expires_at: job.stop.grace_expires_at,
            goods_aboard: job.goods_aboard,
        };
        leave_mode(&state, now).map_err(|refusal| AppError::BusinessRule(refusal.code().into()))
    }

    async fn price(&self, job: &JobView, mode: LeaveMode, now: chrono::DateTime<chrono::Utc>) -> LeaveQuote {
        let p = self.policy;
        let (fee_cents, waiting, past) = match (mode, job.stop.grace_expires_at) {
            (LeaveMode::Drop, _) => (drop_fee_cents(job.payout_cents, p.drop_fee_pct), 0, 0),
            (LeaveMode::Release, Some(deadline)) => (
                0,
                waiting_fee_cents(deadline, now, p.waiting_fee_cents_per_hour),
                minutes_past(deadline, now),
            ),
            (LeaveMode::Release, None) => (0, 0, 0),
        };

        // Only a drop of a job claimed from an offer moves the rate: the rate
        // is over offers, and a release is never the driver's doing.
        let user_id = job.stop.driver_id.inner();
        let stats = self.driver_repo.get_offer_stats(user_id).await.ok().flatten();
        let drops = self.drop_repo.offer_drop_count(user_id).await.unwrap_or(0);
        let weight = p.drop_counts_as_declines;
        let before = stats.and_then(|(seen, claimed)| acceptance_pct(seen, claimed, drops, weight));
        let moves = mode == LeaveMode::Drop
            && before.is_some()
            && self.drop_repo.claimed_from_offer(user_id, job.stop.shipment_id).await.unwrap_or(false);
        let after = match stats {
            Some((seen, claimed)) if moves => acceptance_pct(seen, claimed, drops + 1, weight),
            _ => before,
        };

        LeaveQuote {
            mode,
            fee_cents,
            payout_cents: job.payout_cents,
            fee_pct: p.drop_fee_pct,
            waiting_fee_cents: waiting,
            waiting_fee_cents_per_hour: p.waiting_fee_cents_per_hour,
            minutes_past_grace: past,
            grace_expires_at: job.stop.grace_expires_at,
            acceptance_before: before,
            acceptance_after: after,
        }
    }

    async fn publish_drop(&self, drop: &JobDrop) -> AppResult<()> {
        let event = Event::new("driver-ops", "job.dropped", drop.tenant_id, logisticos_events::payloads::JobDropped {
            tenant_id:    drop.tenant_id,
            driver_id:    drop.driver_id,
            route_id:     drop.route_id,
            shipment_ids: vec![drop.shipment_id],
            reason_code:  drop.reason_code.clone(),
            fee_cents:    drop.fee_cents,
            dropped_at:   drop.dropped_at,
        });
        self.kafka.publish_event(topics::JOB_DROPPED, &event).await.map_err(AppError::Internal)
    }

    /// Admin operation: cancel all pending/in-progress tasks for the given
    /// driver user_id. Resolves drivers.id internally so the caller passes
    /// the JWT user_id (same as identity user_id).
    /// Returns the count of tasks cancelled.
    pub async fn admin_cancel_driver_tasks(
        &self,
        driver_user_id: Uuid,
        tenant_id: &TenantId,
    ) -> AppResult<u64> {
        // Resolve drivers.id from the identity user_id
        let driver_id = self.driver_repo
            .find_by_user_id(driver_user_id)
            .await
            .map_err(AppError::Internal)?
            .map(|d| d.id)
            .unwrap_or_else(|| DriverId::from_uuid(driver_user_id));

        let count = self.task_repo
            .cancel_all_for_driver(&driver_id)
            .await
            .map_err(AppError::Internal)?;

        tracing::info!(
            driver_user_id = %driver_user_id,
            tenant_id = %tenant_id,
            cancelled = count,
            "Admin cancelled driver tasks"
        );
        Ok(count)
    }

    async fn fetch_and_validate_ownership(&self, driver_id: &DriverId, task_id: Uuid) -> AppResult<DriverTask> {
        let task = self.task_repo.find_by_id(task_id).await.map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound { resource: "Task", id: task_id.to_string() })?;

        // driver_id from JWT is user_id; tasks.driver_id is drivers.id (may differ for
        // API-registered drivers). Resolve actual drivers.id by user_id for the comparison.
        let actual_driver_id = self.driver_repo
            .find_by_user_id(driver_id.inner())
            .await
            .map_err(AppError::Internal)?
            .map(|d| d.id)
            .unwrap_or_else(|| driver_id.clone());

        if task.driver_id != actual_driver_id {
            return Err(AppError::Forbidden { resource: "Task".into() });
        }

        Ok(task)
    }
}
