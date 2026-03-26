use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct DispatchQueueRow {
    pub id:                   Uuid,
    pub tenant_id:            Uuid,
    pub shipment_id:          Uuid,
    pub customer_name:        String,
    pub customer_phone:       String,
    pub dest_address_line1:   String,
    pub dest_city:            String,
    pub dest_province:        String,
    pub dest_postal_code:     String,
    pub dest_lat:             Option<f64>,
    pub dest_lng:             Option<f64>,
    pub cod_amount_cents:     Option<i64>,
    pub special_instructions: Option<String>,
    pub service_type:         String,
    pub status:               String,
}

pub struct PgDispatchQueueRepository {
    pool: PgPool,
}

impl PgDispatchQueueRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn upsert(&self, row: &DispatchQueueRow) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO dispatch.dispatch_queue (
                id, tenant_id, shipment_id,
                customer_name, customer_phone,
                dest_address_line1, dest_city, dest_province, dest_postal_code,
                dest_lat, dest_lng,
                cod_amount_cents, special_instructions, service_type, status
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)
            ON CONFLICT (shipment_id) DO NOTHING
            "#,
        )
        .bind(row.id)
        .bind(row.tenant_id)
        .bind(row.shipment_id)
        .bind(&row.customer_name)
        .bind(&row.customer_phone)
        .bind(&row.dest_address_line1)
        .bind(&row.dest_city)
        .bind(&row.dest_province)
        .bind(&row.dest_postal_code)
        .bind(row.dest_lat)
        .bind(row.dest_lng)
        .bind(row.cod_amount_cents)
        .bind(&row.special_instructions)
        .bind(&row.service_type)
        .bind(&row.status)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn find_by_shipment(&self, shipment_id: Uuid) -> anyhow::Result<Option<DispatchQueueRow>> {
        let row = sqlx::query_as::<_, DispatchQueueRow>(
            "SELECT id, tenant_id, shipment_id, customer_name, customer_phone,
                    dest_address_line1, dest_city, dest_province, dest_postal_code,
                    dest_lat, dest_lng, cod_amount_cents, special_instructions, service_type, status
             FROM dispatch.dispatch_queue WHERE shipment_id = $1",
        )
        .bind(shipment_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn list_pending(&self, tenant_id: Uuid) -> anyhow::Result<Vec<DispatchQueueRow>> {
        let rows = sqlx::query_as::<_, DispatchQueueRow>(
            "SELECT id, tenant_id, shipment_id, customer_name, customer_phone,
                    dest_address_line1, dest_city, dest_province, dest_postal_code,
                    dest_lat, dest_lng, cod_amount_cents, special_instructions, service_type, status
             FROM dispatch.dispatch_queue
             WHERE tenant_id = $1 AND status = 'pending'
             ORDER BY queued_at ASC",
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn mark_dispatched(&self, shipment_id: Uuid) -> anyhow::Result<()> {
        let result = sqlx::query(
            "UPDATE dispatch.dispatch_queue
             SET status = 'dispatched', dispatched_at = NOW()
             WHERE shipment_id = $1",
        )
        .bind(shipment_id)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            tracing::warn!(shipment_id = %shipment_id, "mark_dispatched: no row found for shipment_id");
        }
        Ok(())
    }
}
