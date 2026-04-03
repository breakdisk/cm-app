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
        repositories::{TaskRepository, DriverRepository},
    },
};

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
    pub async fn list_my_tasks(&self, driver_id: &DriverId) -> AppResult<Vec<TaskSummary>> {
        let tasks = self.task_repo.list_by_driver(driver_id).await.map_err(AppError::Internal)?;
        Ok(tasks.into_iter()
            .filter(|t| matches!(t.status, TaskStatus::Pending | TaskStatus::InProgress))
            .map(|t| TaskSummary {
                task_id: t.id,
                shipment_id: t.shipment_id,
                sequence: t.sequence as u32,
                status: format!("{:?}", t.status).to_lowercase(),
                task_type: format!("{:?}", t.task_type).to_lowercase(),
                customer_name: t.customer_name.clone(),
                address: format!("{}, {}", t.address.line1, t.address.city),
                cod_amount_cents: t.cod_amount_cents,
            })
            .collect()
        )
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

        task.complete(cmd.pod_id)
            .map_err(|e| AppError::BusinessRule(e.to_string()))?;

        self.task_repo.save(&task).await.map_err(AppError::Internal)?;

        // Publish event — engagement will send delivery notification to customer,
        // payments will process COD reconciliation if applicable.
        let event = Event::new("driver-ops", "delivery.completed", tenant_id.inner(), TaskCompleted {
            task_id: task.id,
            driver_id: driver_id.inner(),
            shipment_id: task.shipment_id,
            tenant_id: tenant_id.inner(),
            pod_id: cmd.pod_id,
            completed_at: task.completed_at.unwrap_or_else(chrono::Utc::now),
        });
        self.kafka.publish_event(topics::DELIVERY_COMPLETED, &event).await
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
        });
        self.kafka.publish_event(topics::DELIVERY_FAILED, &event).await
            .map_err(AppError::Internal)?;

        tracing::info!(task_id = %task.id, driver_id = %driver_id, "Task failed");
        Ok(())
    }

    async fn fetch_and_validate_ownership(&self, driver_id: &DriverId, task_id: Uuid) -> AppResult<DriverTask> {
        let task = self.task_repo.find_by_id(task_id).await.map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound { resource: "Task", id: task_id.to_string() })?;

        if &task.driver_id != driver_id {
            return Err(AppError::Forbidden { resource: "Task".into() });
        }

        Ok(task)
    }
}
