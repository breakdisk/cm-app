//! Postgres implementation of `PaymentIntentRepository`.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::domain::entities::{PaymentIntent, PaymentIntentStatus};
use crate::domain::repositories::PaymentIntentRepository;

pub struct PgPaymentIntentRepository {
    pub pool: PgPool,
}

impl PgPaymentIntentRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn row_to_intent(row: &sqlx::postgres::PgRow) -> PaymentIntent {
    PaymentIntent {
        id: row.get("id"),
        tenant_id: row.get("tenant_id"),
        purpose: row.get("purpose"),
        reference_type: row.get("reference_type"),
        reference_id: row.get("reference_id"),
        amount_cents: row.get("amount_cents"),
        currency: row.get("currency"),
        status: PaymentIntentStatus::parse(row.get::<String, _>("status").as_str())
            .expect("status CHECK constraint guarantees a known value"),
        gateway: row.get("gateway"),
        gateway_order_ref: row.get("gateway_order_ref"),
        gateway_payment_ref: row.get("gateway_payment_ref"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        expires_at: row.get("expires_at"),
    }
}

const INTENT_COLS: &str = "id, tenant_id, purpose, reference_type, reference_id, \
    amount_cents, currency, status, gateway, gateway_order_ref, gateway_payment_ref, \
    created_at, updated_at, expires_at";

#[async_trait]
impl PaymentIntentRepository for PgPaymentIntentRepository {
    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<PaymentIntent>> {
        let query = format!("SELECT {INTENT_COLS} FROM payments.payment_intents WHERE id = $1");
        let row = sqlx::query(&query).bind(id).fetch_optional(&self.pool).await?;
        Ok(row.as_ref().map(row_to_intent))
    }

    async fn find_by_gateway_payment_ref(&self, gateway_payment_ref: &str) -> anyhow::Result<Option<PaymentIntent>> {
        let query = format!("SELECT {INTENT_COLS} FROM payments.payment_intents WHERE gateway_payment_ref = $1");
        let row = sqlx::query(&query).bind(gateway_payment_ref).fetch_optional(&self.pool).await?;
        Ok(row.as_ref().map(row_to_intent))
    }

    async fn save(&self, intent: &PaymentIntent) -> anyhow::Result<()> {
        sqlx::query(
            r#"INSERT INTO payments.payment_intents (
                id, tenant_id, purpose, reference_type, reference_id,
                amount_cents, currency, status, gateway, gateway_order_ref, gateway_payment_ref,
                created_at, updated_at, expires_at
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)
            ON CONFLICT (id) DO UPDATE SET
                status               = EXCLUDED.status,
                gateway_order_ref    = EXCLUDED.gateway_order_ref,
                gateway_payment_ref  = EXCLUDED.gateway_payment_ref,
                updated_at           = EXCLUDED.updated_at"#,
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
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_expired(&self, before: DateTime<Utc>) -> anyhow::Result<Vec<PaymentIntent>> {
        let query = format!(
            "SELECT {INTENT_COLS} FROM payments.payment_intents \
             WHERE status IN ('created','pending') AND expires_at < $1"
        );
        let rows = sqlx::query(&query).bind(before).fetch_all(&self.pool).await?;
        Ok(rows.iter().map(row_to_intent).collect())
    }
}
