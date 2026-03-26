use chrono::NaiveDate;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::entities::{DailyBucket, DeliveryKpis, DriverPerformance, ShipmentEvent};

pub struct AnalyticsDb {
    pool: PgPool,
}

impl AnalyticsDb {
    pub fn new(pool: PgPool) -> Self { Self { pool } }

    // ------------------------------------------------------------------
    // Event store writes (called by Kafka handlers)
    // ------------------------------------------------------------------

    pub async fn insert_event(&self, e: &ShipmentEvent) -> anyhow::Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO analytics.shipment_events (
                id, tenant_id, shipment_id, event_type, driver_id,
                service_type, cod_amount_cents, on_time, delivery_hours, occurred_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            ON CONFLICT (id) DO NOTHING
            "#,
            e.id,
            e.tenant_id,
            e.shipment_id,
            e.event_type,
            e.driver_id,
            e.service_type,
            e.cod_amount_cents,
            e.on_time,
            e.delivery_hours,
            e.occurred_at,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn update_cod_amount(&self, shipment_id: Uuid, amount_cents: i64) -> anyhow::Result<()> {
        sqlx::query!(
            r#"
            UPDATE analytics.shipment_events
            SET cod_amount_cents = $1
            WHERE shipment_id = $2 AND event_type = 'delivered'
            "#,
            amount_cents,
            shipment_id,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // ------------------------------------------------------------------
    // Aggregate queries
    // ------------------------------------------------------------------

    pub async fn delivery_kpis(
        &self,
        tenant_id: Uuid,
        from: NaiveDate,
        to: NaiveDate,
    ) -> anyhow::Result<DeliveryKpis> {
        struct Row {
            total_shipments: Option<i64>,
            delivered:       Option<i64>,
            failed:          Option<i64>,
            cancelled:       Option<i64>,
            on_time_count:   Option<i64>,
            with_eta_count:  Option<i64>,
            avg_hours:       Option<f64>,
            cod_shipments:   Option<i64>,
            cod_collected:   Option<i64>,
        }
        let row = sqlx::query_as!(
            Row,
            r#"
            SELECT
                COUNT(*) FILTER (WHERE event_type = 'created')   AS total_shipments,
                COUNT(*) FILTER (WHERE event_type = 'delivered') AS delivered,
                COUNT(*) FILTER (WHERE event_type = 'failed')    AS failed,
                COUNT(*) FILTER (WHERE event_type = 'cancelled') AS cancelled,
                COUNT(*) FILTER (WHERE event_type = 'delivered' AND on_time = true) AS on_time_count,
                COUNT(*) FILTER (WHERE event_type = 'delivered' AND on_time IS NOT NULL) AS with_eta_count,
                AVG(delivery_hours) FILTER (WHERE event_type = 'delivered') AS avg_hours,
                COUNT(*) FILTER (WHERE event_type = 'delivered' AND cod_amount_cents > 0) AS cod_shipments,
                SUM(cod_amount_cents) FILTER (WHERE event_type = 'delivered') AS cod_collected
            FROM analytics.shipment_events
            WHERE tenant_id = $1
              AND occurred_at >= $2::date::timestamptz
              AND occurred_at <  $3::date::timestamptz + INTERVAL '1 day'
            "#,
            tenant_id,
            from,
            to,
        )
        .fetch_one(&self.pool)
        .await?;

        let total      = row.total_shipments.unwrap_or(0);
        let delivered  = row.delivered.unwrap_or(0);
        let failed     = row.failed.unwrap_or(0);
        let cancelled  = row.cancelled.unwrap_or(0);
        let on_time    = row.on_time_count.unwrap_or(0);
        let with_eta   = row.with_eta_count.unwrap_or(0);
        let cod_ships  = row.cod_shipments.unwrap_or(0);
        let cod_coll   = row.cod_collected.unwrap_or(0);

        let success_rate = if delivered + failed > 0 {
            delivered as f64 / (delivered + failed) as f64 * 100.0
        } else { 0.0 };

        let on_time_rate = if with_eta > 0 {
            on_time as f64 / with_eta as f64 * 100.0
        } else { 0.0 };

        let cod_rate = if cod_ships > 0 {
            cod_coll as f64 / cod_ships as f64 / 100.0  // cents → currency rate
        } else { 0.0 };

        Ok(DeliveryKpis {
            tenant_id,
            from,
            to,
            total_shipments:       total,
            delivered,
            failed,
            cancelled,
            delivery_success_rate: success_rate,
            on_time_rate,
            avg_delivery_hours:    row.avg_hours.unwrap_or(0.0),
            cod_shipments:         cod_ships,
            cod_collected_cents:   cod_coll,
            cod_collection_rate:   cod_rate,
            computed_at:           chrono::Utc::now(),
        })
    }

    pub async fn daily_timeseries(
        &self,
        tenant_id: Uuid,
        from: NaiveDate,
        to: NaiveDate,
    ) -> anyhow::Result<Vec<DailyBucket>> {
        struct Row {
            date:              Option<NaiveDate>,
            shipments:         Option<i64>,
            delivered:         Option<i64>,
            failed:            Option<i64>,
            cod_collected:     Option<i64>,
        }
        let rows = sqlx::query_as!(
            Row,
            r#"
            SELECT
                DATE(occurred_at AT TIME ZONE 'Asia/Manila') AS date,
                COUNT(*) FILTER (WHERE event_type = 'created')   AS shipments,
                COUNT(*) FILTER (WHERE event_type = 'delivered') AS delivered,
                COUNT(*) FILTER (WHERE event_type = 'failed')    AS failed,
                COALESCE(SUM(cod_amount_cents) FILTER (WHERE event_type = 'delivered'), 0) AS cod_collected
            FROM analytics.shipment_events
            WHERE tenant_id = $1
              AND occurred_at >= $2::date::timestamptz
              AND occurred_at <  $3::date::timestamptz + INTERVAL '1 day'
            GROUP BY DATE(occurred_at AT TIME ZONE 'Asia/Manila')
            ORDER BY date ASC
            "#,
            tenant_id,
            from,
            to,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().filter_map(|r| {
            let date      = r.date?;
            let ships     = r.shipments.unwrap_or(0);
            let delivered = r.delivered.unwrap_or(0);
            let failed    = r.failed.unwrap_or(0);
            let success   = if delivered + failed > 0 {
                delivered as f64 / (delivered + failed) as f64 * 100.0
            } else { 0.0 };
            Some(DailyBucket {
                date,
                shipments: ships,
                delivered,
                failed,
                success_rate: success,
                cod_collected_cents: r.cod_collected.unwrap_or(0),
            })
        }).collect())
    }

    pub async fn driver_performance(
        &self,
        tenant_id: Uuid,
        from: NaiveDate,
        to: NaiveDate,
        limit: i64,
    ) -> anyhow::Result<Vec<DriverPerformance>> {
        struct Row {
            driver_id:        Option<Uuid>,
            total_deliveries: Option<i64>,
            successful:       Option<i64>,
            failed:           Option<i64>,
            avg_hours:        Option<f64>,
            cod_collected:    Option<i64>,
        }
        let rows = sqlx::query_as!(
            Row,
            r#"
            SELECT
                driver_id,
                COUNT(*) FILTER (WHERE event_type IN ('delivered','failed')) AS total_deliveries,
                COUNT(*) FILTER (WHERE event_type = 'delivered') AS successful,
                COUNT(*) FILTER (WHERE event_type = 'failed')    AS failed,
                AVG(delivery_hours) FILTER (WHERE event_type = 'delivered') AS avg_hours,
                COALESCE(SUM(cod_amount_cents) FILTER (WHERE event_type = 'delivered'), 0) AS cod_collected
            FROM analytics.shipment_events
            WHERE tenant_id = $1
              AND driver_id IS NOT NULL
              AND occurred_at >= $2::date::timestamptz
              AND occurred_at <  $3::date::timestamptz + INTERVAL '1 day'
            GROUP BY driver_id
            ORDER BY successful DESC
            LIMIT $4
            "#,
            tenant_id, from, to, limit,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().filter_map(|r| {
            let driver_id = r.driver_id?;
            let total     = r.total_deliveries.unwrap_or(0);
            let success   = r.successful.unwrap_or(0);
            let failed    = r.failed.unwrap_or(0);
            let rate      = if total > 0 { success as f64 / total as f64 * 100.0 } else { 0.0 };
            Some(DriverPerformance {
                driver_id,
                driver_name: None, // enriched by client via identity service
                total_deliveries: total,
                successful: success,
                failed,
                success_rate: rate,
                avg_delivery_hours: r.avg_hours.unwrap_or(0.0),
                cod_collected_cents: r.cod_collected.unwrap_or(0),
            })
        }).collect())
    }
}
