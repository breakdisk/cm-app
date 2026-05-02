use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct DispatchQueueRow {
    pub id:                   Uuid,
    pub tenant_id:            Uuid,
    pub shipment_id:          Uuid,
    pub customer_id:          Uuid,  // From ShipmentCreated event
    pub customer_name:        String,
    pub customer_phone:       String,
    pub customer_email:       Option<String>,
    pub tracking_number:      Option<String>,
    pub dest_address_line1:   String,
    pub dest_city:            String,
    pub dest_province:        String,
    pub dest_postal_code:     String,
    pub dest_lat:             Option<f64>,
    pub dest_lng:             Option<f64>,
    pub origin_address_line1: String,
    pub origin_city:          String,
    pub origin_province:      String,
    pub origin_postal_code:   String,
    pub origin_lat:           Option<f64>,
    pub origin_lng:           Option<f64>,
    pub cod_amount_cents:     Option<i64>,
    pub special_instructions: Option<String>,
    pub service_type:         String,
    pub status:               String,
    /// Count of failed auto-dispatch attempts. Non-zero means the initial
    /// auto-assign (from a customer/merchant booking) could not find a
    /// driver — the row is parked here waiting for ops intervention.
    #[serde(default)]
    pub auto_dispatch_attempts: i32,
    #[serde(default)]
    pub last_dispatch_error:    Option<String>,
    #[serde(default)]
    pub last_attempt_at:        Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub queued_at:              Option<chrono::DateTime<chrono::Utc>>,
    /// NULL while the row is `pending`; set by mark_dispatched when the
    /// shipment is auto-assigned or manually dispatched. Surfaced so the
    /// admin console can show "Dispatched" rows distinct from pending.
    pub dispatched_at:          Option<chrono::DateTime<chrono::Utc>>,
}

#[async_trait]
pub trait DispatchQueueRepository: Send + Sync {
    async fn upsert(&self, row: &DispatchQueueRow) -> anyhow::Result<()>;
    async fn find_by_shipment(&self, shipment_id: Uuid) -> anyhow::Result<Option<DispatchQueueRow>>;
    async fn list_pending(&self, tenant_id: Uuid) -> anyhow::Result<Vec<DispatchQueueRow>>;
    /// Filter by status — pass `Some("dispatched")` for the Dispatched tab,
    /// `Some("pending")` for the queue, or `None` for the All tab.
    async fn list_by_status(
        &self,
        tenant_id: Uuid,
        status: Option<&str>,
    ) -> anyhow::Result<Vec<DispatchQueueRow>>;
    async fn mark_dispatched(&self, shipment_id: Uuid) -> anyhow::Result<()>;
    /// Increment attempt counter and record the reason. Called by the
    /// shipment consumer when quick_dispatch fails so the admin console
    /// can visually flag shipments needing manual intervention.
    async fn record_failed_attempt(&self, shipment_id: Uuid, error: &str) -> anyhow::Result<()>;

    /// Re-queues a delivery-failed shipment for another dispatch attempt.
    /// Resets status back to 'pending' and increments auto_dispatch_attempts
    /// so operators know this is a retry. No-op if the row doesn't exist.
    async fn reset_to_pending(&self, shipment_id: Uuid) -> anyhow::Result<()>;
}

pub struct PgDispatchQueueRepository {
    pool: PgPool,
}

impl PgDispatchQueueRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DispatchQueueRepository for PgDispatchQueueRepository {
    async fn upsert(&self, row: &DispatchQueueRow) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO dispatch.dispatch_queue (
                id, tenant_id, shipment_id, customer_id,
                customer_name, customer_phone, customer_email, tracking_number,
                dest_address_line1, dest_city, dest_province, dest_postal_code,
                dest_lat, dest_lng,
                origin_address_line1, origin_city, origin_province, origin_postal_code,
                origin_lat, origin_lng,
                cod_amount_cents, special_instructions, service_type, status
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24)
            ON CONFLICT (shipment_id) DO NOTHING
            "#,
        )
        .bind(row.id)
        .bind(row.tenant_id)
        .bind(row.shipment_id)
        .bind(row.customer_id)
        .bind(&row.customer_name)
        .bind(&row.customer_phone)
        .bind(&row.customer_email)
        .bind(&row.tracking_number)
        .bind(&row.dest_address_line1)
        .bind(&row.dest_city)
        .bind(&row.dest_province)
        .bind(&row.dest_postal_code)
        .bind(row.dest_lat)
        .bind(row.dest_lng)
        .bind(&row.origin_address_line1)
        .bind(&row.origin_city)
        .bind(&row.origin_province)
        .bind(&row.origin_postal_code)
        .bind(row.origin_lat)
        .bind(row.origin_lng)
        .bind(row.cod_amount_cents)
        .bind(&row.special_instructions)
        .bind(&row.service_type)
        .bind(&row.status)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn find_by_shipment(&self, shipment_id: Uuid) -> anyhow::Result<Option<DispatchQueueRow>> {
        let row = sqlx::query_as::<_, DispatchQueueRow>(
            "SELECT id, tenant_id, shipment_id, customer_id, customer_name, customer_phone,
                    customer_email, tracking_number,
                    dest_address_line1, dest_city, dest_province, dest_postal_code,
                    dest_lat, dest_lng,
                    origin_address_line1, origin_city, origin_province, origin_postal_code,
                    origin_lat, origin_lng,
                    cod_amount_cents, special_instructions, service_type, status,
                    auto_dispatch_attempts, last_dispatch_error, last_attempt_at,
                    queued_at, dispatched_at
             FROM dispatch.dispatch_queue WHERE shipment_id = $1",
        )
        .bind(shipment_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    async fn list_pending(&self, tenant_id: Uuid) -> anyhow::Result<Vec<DispatchQueueRow>> {
        self.list_by_status(tenant_id, Some("pending")).await
    }

    async fn list_by_status(
        &self,
        tenant_id: Uuid,
        status: Option<&str>,
    ) -> anyhow::Result<Vec<DispatchQueueRow>> {
        let base = "SELECT id, tenant_id, shipment_id, customer_id, customer_name, customer_phone,
                           customer_email, tracking_number,
                           dest_address_line1, dest_city, dest_province, dest_postal_code,
                           dest_lat, dest_lng,
                           origin_address_line1, origin_city, origin_province, origin_postal_code,
                           origin_lat, origin_lng,
                           cod_amount_cents, special_instructions, service_type, status,
                           auto_dispatch_attempts, last_dispatch_error, last_attempt_at,
                           queued_at, dispatched_at
                    FROM dispatch.dispatch_queue
                    WHERE tenant_id = $1";
        // Pending stays FIFO (oldest first) so dispatchers naturally work
        // the queue head; dispatched/all show newest activity first since
        // that's the more useful "what just happened" view.
        let rows = match status {
            Some("pending") => sqlx::query_as::<_, DispatchQueueRow>(
                &format!("{base} AND status = $2 ORDER BY queued_at ASC"),
            )
            .bind(tenant_id)
            .bind("pending")
            .fetch_all(&self.pool)
            .await?,
            Some(s) => sqlx::query_as::<_, DispatchQueueRow>(
                &format!("{base} AND status = $2 ORDER BY COALESCE(dispatched_at, queued_at) DESC"),
            )
            .bind(tenant_id)
            .bind(s)
            .fetch_all(&self.pool)
            .await?,
            None => sqlx::query_as::<_, DispatchQueueRow>(
                &format!("{base} ORDER BY COALESCE(dispatched_at, queued_at) DESC"),
            )
            .bind(tenant_id)
            .fetch_all(&self.pool)
            .await?,
        };
        Ok(rows)
    }

    async fn mark_dispatched(&self, shipment_id: Uuid) -> anyhow::Result<()> {
        let result = sqlx::query(
            "UPDATE dispatch.dispatch_queue
             SET status = 'dispatched', dispatched_at = NOW()
             WHERE shipment_id = $1",
        )
        .bind(shipment_id)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            anyhow::bail!("mark_dispatched: no dispatch_queue row found for shipment_id {}", shipment_id);
        }
        Ok(())
    }

    async fn record_failed_attempt(&self, shipment_id: Uuid, error: &str) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE dispatch.dispatch_queue
             SET auto_dispatch_attempts = auto_dispatch_attempts + 1,
                 last_dispatch_error    = $2,
                 last_attempt_at        = NOW()
             WHERE shipment_id = $1",
        )
        .bind(shipment_id)
        .bind(error)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn reset_to_pending(&self, shipment_id: Uuid) -> anyhow::Result<()> {
        // Reset a delivery-failed shipment back to pending so it can be
        // re-dispatched. Increments the attempt counter so operators can
        // see how many times a shipment has cycled through.
        sqlx::query(
            "UPDATE dispatch.dispatch_queue
             SET status                 = 'pending',
                 dispatched_at          = NULL,
                 auto_dispatch_attempts = auto_dispatch_attempts + 1,
                 last_dispatch_error    = 'delivery failed — requeued for retry',
                 last_attempt_at        = NOW()
             WHERE shipment_id = $1",
        )
        .bind(shipment_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
