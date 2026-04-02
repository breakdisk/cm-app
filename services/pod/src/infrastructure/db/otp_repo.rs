use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;
use crate::domain::{entities::OtpCode, repositories::OtpRepository};

pub struct PgOtpRepository {
    pool: PgPool,
}

impl PgOtpRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[derive(sqlx::FromRow)]
struct OtpRow {
    id:          Uuid,
    tenant_id:   Uuid,
    shipment_id: Uuid,
    phone:       String,
    code_hash:   String,
    is_used:     bool,
    expires_at:  chrono::DateTime<chrono::Utc>,
    created_at:  chrono::DateTime<chrono::Utc>,
}

impl From<OtpRow> for OtpCode {
    fn from(r: OtpRow) -> Self {
        OtpCode {
            id: r.id,
            tenant_id: r.tenant_id,
            shipment_id: r.shipment_id,
            phone: r.phone,
            code_hash: r.code_hash,
            is_used: r.is_used,
            expires_at: r.expires_at,
            created_at: r.created_at,
        }
    }
}

#[async_trait]
impl OtpRepository for PgOtpRepository {
    async fn find_active_by_shipment(&self, shipment_id: Uuid, tenant_id: Uuid) -> anyhow::Result<Option<OtpCode>> {
        let row = sqlx::query_as::<_, OtpRow>(
            r#"SELECT id, tenant_id, shipment_id, phone, code_hash, is_used, expires_at, created_at
               FROM pod.otp_codes
               WHERE shipment_id = $1
                 AND tenant_id = $2
                 AND is_used = false
                 AND expires_at > NOW()
               ORDER BY created_at DESC
               LIMIT 1"#,
        )
        .bind(shipment_id)
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(OtpCode::from))
    }

    async fn save(&self, otp: &OtpCode) -> anyhow::Result<()> {
        sqlx::query(
            r#"INSERT INTO pod.otp_codes
                   (id, tenant_id, shipment_id, phone, code_hash, is_used, expires_at, created_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
               ON CONFLICT (id) DO UPDATE SET
                   is_used = EXCLUDED.is_used"#,
        )
        .bind(otp.id)
        .bind(otp.tenant_id)
        .bind(otp.shipment_id)
        .bind(&otp.phone)
        .bind(&otp.code_hash)
        .bind(otp.is_used)
        .bind(otp.expires_at)
        .bind(otp.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
