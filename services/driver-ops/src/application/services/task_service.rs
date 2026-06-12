use std::sync::Arc;
use logisticos_errors::{AppError, AppResult};
use logisticos_types::{DriverId, TenantId};
use logisticos_events::{producer::KafkaProducer, topics, envelope::Event};
use uuid::Uuid;

use crate::{
    application::commands::{StartTaskCommand, CompleteTaskCommand, FailTaskCommand, TaskSummary},
    domain::{
        entities::{DriverTask, TaskStatus, TaskType},
        events::{TaskCompleted, TaskFailed},
        repositories::{DailyEarning, DriverRepository, EarningEntry, TaskRepository, TenantTaskSummary},
    },
};

/// Earnings view returned by `GET /v1/drivers/me/earnings`.
/// `today_cents` / `week_cents` are derived from `daily` server-side so the
/// app renders the summary card without re-aggregating.
#[derive(Debug, serde::Serialize)]
pub struct EarningsView {
    pub today_cents: i64,
    pub week_cents:  i64,
    pub daily:       Vec<DailyEarning>,
    pub entries:     Vec<EarningEntry>,
}

pub struct TaskService {
    task_repo: Arc<dyn TaskRepository>,
    driver_repo: Arc<dyn DriverRepository>,
    kafka: Arc<KafkaProducer>,
}

impl TaskService {
    pub fn new(
        task_repo: Arc<dyn TaskRepository>,
        driver_repo: Arc<dyn DriverRepository>,
        kafka: Arc<KafkaProducer>,
    ) -> Self {
        Self { task_repo, driver_repo, kafka }
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

        Ok(EarningsView { today_cents, week_cents, daily, entries })
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
        let has_cod = t.cod_amount_cents.unwrap_or(0) > 0;
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
            // signature. OTP only when COD is collected (verifies recipient
            // received cash). Persisted requires_* columns will replace this
            // heuristic once dispatch propagates the per-shipment policy.
            requires_photo:     true,
            requires_signature: is_delivery,
            requires_otp:       is_delivery && has_cod,
        }
    }

    pub async fn start_task(&self, driver_id: &DriverId, cmd: StartTaskCommand) -> AppResult<()> {
        let mut task = self.fetch_and_validate_ownership(driver_id, cmd.task_id).await?;

        if task.status != TaskStatus::Pending {
            return Err(AppError::BusinessRule("Can only start a pending task".into()));
        }

        task.start();
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
        });
        self.kafka.publish_event(topics::DELIVERY_FAILED, &event).await
            .map_err(AppError::Internal)?;

        tracing::info!(task_id = %task.id, driver_id = %driver_id, "Task failed");
        Ok(())
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
