//! PostgreSQL-backed rule repository.
//!
//! Rules are stored with trigger/conditions/actions as JSONB columns for
//! schema flexibility. Loaded at startup into the in-memory RuleRepository
//! and hot-reloaded on `POST /v1/rules/reload`.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::entities::rule::{
    AutomationRule, RuleAction, RuleCondition, RuleTrigger,
};

/// Internal row type for reading rules from the database.
#[derive(sqlx::FromRow)]
struct RuleRow {
    id:          Uuid,
    tenant_id:   Uuid,
    name:        String,
    description: Option<String>,  // nullable in DB; mapped to String via unwrap_or_default
    is_active:   bool,
    trigger_def: serde_json::Value,
    conditions:  serde_json::Value,
    actions:     serde_json::Value,
    priority:    i32,
    created_at:  DateTime<Utc>,
}

pub struct PgRuleRepository {
    pool: PgPool,
}

impl PgRuleRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Load all active rules for a tenant (and nil-UUID platform rules).
    pub async fn load_for_tenant(&self, tenant_id: Uuid) -> anyhow::Result<Vec<AutomationRule>> {
        let rows = sqlx::query_as::<_, RuleRow>(
            r#"
            SELECT id, tenant_id, name, description, is_active,
                   trigger_def, conditions, actions, priority, created_at
            FROM business_logic.rules
            WHERE (tenant_id = $1 OR tenant_id = '00000000-0000-0000-0000-000000000000')
              AND is_active = true
            ORDER BY priority ASC
            "#
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await?;

        let mut rules = Vec::with_capacity(rows.len());
        for row in rows {
            let trigger: RuleTrigger = serde_json::from_value(row.trigger_def)?;
            let conditions: Vec<RuleCondition> = serde_json::from_value(row.conditions)?;
            let actions: Vec<RuleAction> = serde_json::from_value(row.actions)?;
            rules.push(AutomationRule {
                id: row.id,
                tenant_id: row.tenant_id,
                name: row.name,
                description: row.description.unwrap_or_default(),
                is_active: row.is_active,
                trigger,
                conditions,
                actions,
                priority: row.priority as u32,
                created_at: row.created_at,
            });
        }
        Ok(rules)
    }

    /// Load ALL active rules (platform + all tenants) for initial seeding.
    pub async fn load_all(&self) -> anyhow::Result<Vec<AutomationRule>> {
        let rows = sqlx::query_as::<_, RuleRow>(
            r#"
            SELECT id, tenant_id, name, description, is_active,
                   trigger_def, conditions, actions, priority, created_at
            FROM business_logic.rules
            ORDER BY priority ASC
            "#
        )
        .fetch_all(&self.pool)
        .await?;

        let mut rules = Vec::with_capacity(rows.len());
        for row in rows {
            let trigger: RuleTrigger = serde_json::from_value(row.trigger_def)?;
            let conditions: Vec<RuleCondition> = serde_json::from_value(row.conditions)?;
            let actions: Vec<RuleAction> = serde_json::from_value(row.actions)?;
            rules.push(AutomationRule {
                id: row.id,
                tenant_id: row.tenant_id,
                name: row.name,
                description: row.description.unwrap_or_default(),
                is_active: row.is_active,
                trigger,
                conditions,
                actions,
                priority: row.priority as u32,
                created_at: row.created_at,
            });
        }
        Ok(rules)
    }

    pub async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<AutomationRule>> {
        let row = sqlx::query_as::<_, RuleRow>(
            r#"
            SELECT id, tenant_id, name, description, is_active,
                   trigger_def, conditions, actions, priority, created_at
            FROM business_logic.rules
            WHERE id = $1
            "#
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            let trigger: RuleTrigger = serde_json::from_value(row.trigger_def)?;
            let conditions: Vec<RuleCondition> = serde_json::from_value(row.conditions)?;
            let actions: Vec<RuleAction> = serde_json::from_value(row.actions)?;
            Ok(Some(AutomationRule {
                id: row.id,
                tenant_id: row.tenant_id,
                name: row.name,
                description: row.description.unwrap_or_default(),
                is_active: row.is_active,
                trigger,
                conditions,
                actions,
                priority: row.priority as u32,
                created_at: row.created_at,
            }))
        } else {
            Ok(None)
        }
    }

    pub async fn create(&self, rule: &AutomationRule) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO business_logic.rules
                (id, tenant_id, name, description, is_active,
                 trigger_def, conditions, actions, priority, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            "#
        )
        .bind(rule.id)
        .bind(rule.tenant_id)
        .bind(&rule.name)
        .bind(&rule.description)
        .bind(rule.is_active)
        .bind(serde_json::to_value(&rule.trigger)?)
        .bind(serde_json::to_value(&rule.conditions)?)
        .bind(serde_json::to_value(&rule.actions)?)
        .bind(rule.priority as i32)
        .bind(rule.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn update(&self, rule: &AutomationRule) -> anyhow::Result<bool> {
        let result = sqlx::query(
            r#"
            UPDATE business_logic.rules
            SET name        = $1,
                description = $2,
                is_active   = $3,
                trigger_def = $4,
                conditions  = $5,
                actions     = $6,
                priority    = $7
            WHERE id = $8 AND tenant_id = $9
            "#
        )
        .bind(&rule.name)
        .bind(&rule.description)
        .bind(rule.is_active)
        .bind(serde_json::to_value(&rule.trigger)?)
        .bind(serde_json::to_value(&rule.conditions)?)
        .bind(serde_json::to_value(&rule.actions)?)
        .bind(rule.priority as i32)
        .bind(rule.id)
        .bind(rule.tenant_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn set_active(&self, id: Uuid, tenant_id: Uuid, is_active: bool) -> anyhow::Result<bool> {
        let result = sqlx::query(
            "UPDATE business_logic.rules SET is_active = $1 WHERE id = $2 AND tenant_id = $3"
        )
        .bind(is_active).bind(id).bind(tenant_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn delete(&self, id: Uuid, tenant_id: Uuid) -> anyhow::Result<bool> {
        let result = sqlx::query(
            "DELETE FROM business_logic.rules WHERE id = $1 AND tenant_id = $2"
        )
        .bind(id).bind(tenant_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Append a rule execution record to the audit log.
    pub async fn log_execution(
        &self,
        rule_id: Uuid,
        tenant_id: Uuid,
        kafka_topic: &str,
        shipment_id: Option<Uuid>,
        conditions_passed: bool,
        actions_executed: &[String],
        outcome: &str,
        error_message: Option<&str>,
        fired_at: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO business_logic.rule_executions
                (id, rule_id, tenant_id, kafka_topic, shipment_id,
                 conditions_passed, actions_executed, outcome, error_message, fired_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            "#
        )
        .bind(Uuid::new_v4())
        .bind(rule_id)
        .bind(tenant_id)
        .bind(kafka_topic)
        .bind(shipment_id)
        .bind(conditions_passed)
        .bind(serde_json::to_value(actions_executed).unwrap_or_default())
        .bind(outcome)
        .bind(error_message)
        .bind(fired_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_executions(
        &self,
        rule_id: Uuid,
        limit: i64,
        cursor_fired_at: Option<DateTime<Utc>>,
    ) -> anyhow::Result<Vec<RuleExecutionRow>> {
        let rows = if let Some(cursor) = cursor_fired_at {
            sqlx::query_as::<_, RuleExecutionRow>(
                r#"
                SELECT id, rule_id, tenant_id, kafka_topic, shipment_id,
                       conditions_passed, actions_executed, outcome, error_message, fired_at
                FROM business_logic.rule_executions
                WHERE rule_id = $1 AND fired_at < $2
                ORDER BY fired_at DESC
                LIMIT $3
                "#
            )
            .bind(rule_id).bind(cursor).bind(limit)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query_as::<_, RuleExecutionRow>(
                r#"
                SELECT id, rule_id, tenant_id, kafka_topic, shipment_id,
                       conditions_passed, actions_executed, outcome, error_message, fired_at
                FROM business_logic.rule_executions
                WHERE rule_id = $1
                ORDER BY fired_at DESC
                LIMIT $2
                "#
            )
            .bind(rule_id).bind(limit)
            .fetch_all(&self.pool)
            .await?
        };
        Ok(rows)
    }
}

#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct RuleExecutionRow {
    pub id: Uuid,
    pub rule_id: Uuid,
    pub tenant_id: Uuid,
    pub kafka_topic: String,
    pub shipment_id: Option<Uuid>,
    pub conditions_passed: bool,
    pub actions_executed: serde_json::Value,
    pub outcome: String,
    pub error_message: Option<String>,
    pub fired_at: DateTime<Utc>,
}
