//! A home move's survey photos: condition evidence and access constraints.

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct SurveyPhoto {
    pub id: Uuid,
    pub shipment_id: Uuid,
    /// condition | access
    pub kind: String,
    pub room: Option<String>,
    pub caption: String,
    #[serde(skip_serializing)]
    pub object_key: String,
    pub device_timestamp: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    /// Viewable for an hour; filled when read.
    pub url: Option<String>,
}

pub struct NewPhoto<'a> {
    pub shipment_id: Uuid,
    pub tenant_id: Uuid,
    pub uploaded_by: Uuid,
    pub kind: &'a str,
    pub room: Option<&'a str>,
    pub caption: &'a str,
    pub object_key: &'a str,
    pub content_type: &'a str,
    pub size_bytes: i64,
    pub device_timestamp: Option<DateTime<Utc>>,
}

/// Recorded once per object; a retried upload returns the photo it made.
pub async fn insert(pool: &PgPool, p: &NewPhoto<'_>) -> anyhow::Result<SurveyPhoto> {
    let row = sqlx::query(
        r#"INSERT INTO order_intake.home_survey_photos
               (shipment_id, tenant_id, uploaded_by, kind, room, caption, object_key, content_type, size_bytes, device_timestamp)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
           ON CONFLICT (object_key) DO UPDATE SET object_key = EXCLUDED.object_key
           RETURNING id, shipment_id, kind, room, caption, object_key, device_timestamp, created_at"#,
    )
    .bind(p.shipment_id)
    .bind(p.tenant_id)
    .bind(p.uploaded_by)
    .bind(p.kind)
    .bind(p.room)
    .bind(p.caption)
    .bind(p.object_key)
    .bind(p.content_type)
    .bind(p.size_bytes)
    .bind(p.device_timestamp)
    .fetch_one(pool)
    .await?;
    Ok(photo(&row))
}

pub async fn list(pool: &PgPool, tenant_id: Uuid, shipment_id: Uuid) -> anyhow::Result<Vec<SurveyPhoto>> {
    let rows = sqlx::query(
        "SELECT id, shipment_id, kind, room, caption, object_key, device_timestamp, created_at
           FROM order_intake.home_survey_photos
          WHERE tenant_id = $1 AND shipment_id = $2
          ORDER BY created_at",
    )
    .bind(tenant_id)
    .bind(shipment_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.iter().map(photo).collect())
}

pub async fn count(pool: &PgPool, shipment_id: Uuid) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar("SELECT COUNT(*) FROM order_intake.home_survey_photos WHERE shipment_id = $1")
        .bind(shipment_id)
        .fetch_one(pool)
        .await?)
}

fn photo(r: &sqlx::postgres::PgRow) -> SurveyPhoto {
    SurveyPhoto {
        id: r.get("id"),
        shipment_id: r.get("shipment_id"),
        kind: r.get("kind"),
        room: r.get("room"),
        caption: r.get("caption"),
        object_key: r.get("object_key"),
        device_timestamp: r.get("device_timestamp"),
        created_at: r.get("created_at"),
        url: None,
    }
}
