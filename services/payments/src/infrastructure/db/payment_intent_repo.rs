//! Postgres implementation of `PaymentIntentRepository`.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::entities::{PaymentIntent, PaymentIntentStatus};
use crate::domain::repositories::PaymentIntentRepository;

pub struct PgPaymentIntentRepository {
    pool: PgPool,
}

impl PgPaymentIntentRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(sqlx::FromRow)]
struct PaymentIntentRow {
    id: Uuid,
    tenant_id: Uuid,
    purpose: String,
    reference_type: String,
    reference_id: Uuid,
    amount_cents: i64,
    currency: String,
    status: String,
    gateway: String,
    gateway_order_ref: Option<String>,
    gateway_payment_ref: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    refund_requested_at: Option<DateTime<Utc>>,
}

impl TryFrom<PaymentIntentRow> for PaymentIntent {
    type Error = anyhow::Error;

    fn try_from(row: PaymentIntentRow) -> Result<Self, Self::Error> {
        let status = PaymentIntentStatus::parse(&row.status)
            .ok_or_else(|| anyhow::anyhow!("unknown payment_intents.status: {}", row.status))?;
        Ok(PaymentIntent {
            id: row.id,
            tenant_id: row.tenant_id,
            purpose: row.purpose,
            reference_type: row.reference_type,
            reference_id: row.reference_id,
            amount_cents: row.amount_cents,
            currency: row.currency,
            status,
            gateway: row.gateway,
            gateway_order_ref: row.gateway_order_ref,
            gateway_payment_ref: row.gateway_payment_ref,
            created_at: row.created_at,
            updated_at: row.updated_at,
            expires_at: row.expires_at,
            refund_requested_at: row.refund_requested_at,
        })
    }
}

const INTENT_COLS: &str = "id, tenant_id, purpose, reference_type, reference_id, \
    amount_cents, currency, status, gateway, gateway_order_ref, gateway_payment_ref, \
    created_at, updated_at, expires_at, refund_requested_at";

#[async_trait]
impl PaymentIntentRepository for PgPaymentIntentRepository {
    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<PaymentIntent>> {
        let query = format!("SELECT {INTENT_COLS} FROM payments.payment_intents WHERE id = $1");
        let row = sqlx::query_as::<_, PaymentIntentRow>(&query)
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        row.map(PaymentIntent::try_from).transpose()
    }

    async fn find_by_gateway_payment_ref(&self, gateway_payment_ref: &str) -> anyhow::Result<Option<PaymentIntent>> {
        let query = format!("SELECT {INTENT_COLS} FROM payments.payment_intents WHERE gateway_payment_ref = $1");
        let row = sqlx::query_as::<_, PaymentIntentRow>(&query)
            .bind(gateway_payment_ref)
            .fetch_optional(&self.pool)
            .await?;
        row.map(PaymentIntent::try_from).transpose()
    }

    async fn save(&self, intent: &PaymentIntent) -> anyhow::Result<()> {
        sqlx::query(
            r#"INSERT INTO payments.payment_intents (
                id, tenant_id, purpose, reference_type, reference_id,
                amount_cents, currency, status, gateway, gateway_order_ref, gateway_payment_ref,
                created_at, updated_at, expires_at, refund_requested_at
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)
            ON CONFLICT (id) DO UPDATE SET
                status               = EXCLUDED.status,
                gateway_order_ref    = EXCLUDED.gateway_order_ref,
                gateway_payment_ref  = EXCLUDED.gateway_payment_ref,
                updated_at           = EXCLUDED.updated_at,
                refund_requested_at  = EXCLUDED.refund_requested_at"#,
        )
        .bind(intent.id)
        .bind(intent.tenant_id)
        .bind(&intent.purpose)
        .bind(&intent.reference_type)
        .bind(intent.reference_id)
        .bind(intent.amount_cents)
        .bind(&intent.currency)
        .bind(intent.status.as_str())
        .bind(&intent.gateway)
        .bind(intent.gateway_order_ref.as_deref())
        .bind(intent.gateway_payment_ref.as_deref())
        .bind(intent.created_at)
        .bind(intent.updated_at)
        .bind(intent.expires_at)
        .bind(intent.refund_requested_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_expired(&self, before: DateTime<Utc>) -> anyhow::Result<Vec<PaymentIntent>> {
        let query = format!(
            "SELECT {INTENT_COLS} FROM payments.payment_intents \
             WHERE status IN ('created','pending') AND expires_at < $1"
        );
        let rows = sqlx::query_as::<_, PaymentIntentRow>(&query)
            .bind(before)
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter().map(PaymentIntent::try_from).collect()
    }

    async fn find_captured_by_reference(
        &self,
        purpose: &str,
        reference_type: &str,
        reference_id: Uuid,
    ) -> anyhow::Result<Option<PaymentIntent>> {
        let query = format!(
            "SELECT {INTENT_COLS} FROM payments.payment_intents \
             WHERE purpose = $1 AND reference_type = $2 AND reference_id = $3 AND status = 'captured'"
        );
        let row = sqlx::query_as::<_, PaymentIntentRow>(&query)
            .bind(purpose)
            .bind(reference_type)
            .bind(reference_id)
            .fetch_optional(&self.pool)
            .await?;
        row.map(PaymentIntent::try_from).transpose()
    }

    async fn mark_refund_requested(&self, id: Uuid) -> anyhow::Result<()> {
        // COALESCE keeps this idempotent — a redelivered cancellation event
        // (or a duplicate call) must not push the clock forward and reset
        // how long the obligation has been outstanding.
        sqlx::query(
            "UPDATE payments.payment_intents \
             SET refund_requested_at = COALESCE(refund_requested_at, NOW()) \
             WHERE id = $1",
        )
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn claim_for_refund(&self, id: Uuid) -> anyhow::Result<bool> {
        // A claim expires. If the process holding it died between the claim
        // and the gateway call, or failed to revert it, the row would sit in
        // `refunding` forever -- invisible to the sweep, customer still
        // charged. The lease far exceeds any in-flight gateway call (the NI
        // client times out at 30s), so reclaiming cannot race a live call.
        // The whole race is decided by Postgres as part of this single
        // write: a concurrent claim on the same row makes the WHERE match
        // zero rows for everyone but the first UPDATE to commit, rather than
        // two callers both reading `status = 'captured'` and both proceeding.
        let result = sqlx::query(
            "UPDATE payments.payment_intents \
             SET status = 'refunding', updated_at = NOW() \
             WHERE id = $1 \n               AND (status = 'captured' \n                    OR (status = 'refunding' AND updated_at < NOW() - INTERVAL '15 minutes'))",
        )
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn list_pending_refunds(&self) -> anyhow::Result<Vec<PaymentIntent>> {
        let query = format!(
            "SELECT {INTENT_COLS} FROM payments.payment_intents \
             WHERE refund_requested_at IS NOT NULL \n               AND (status = 'captured' \n                    OR (status = 'refunding' AND updated_at < NOW() - INTERVAL '15 minutes'))"
        );
        let rows = sqlx::query_as::<_, PaymentIntentRow>(&query)
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter().map(PaymentIntent::try_from).collect()
    }
}
