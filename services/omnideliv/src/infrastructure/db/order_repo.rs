use async_trait::async_trait;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::domain::entities::{LegStatus, Order, OrderStatus, VendorLeg};
use crate::domain::repositories::OrderRepository;

pub struct PgOrderRepository { pool: PgPool }

impl PgOrderRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

fn order_status(s: &str) -> anyhow::Result<OrderStatus> {
    Ok(match s {
        "placed"           => OrderStatus::Placed,
        "awaiting_courier" => OrderStatus::AwaitingCourier,
        "collecting"       => OrderStatus::Collecting,
        "delivering"       => OrderStatus::Delivering,
        "delivered"        => OrderStatus::Delivered,
        "cancelled"        => OrderStatus::Cancelled,
        other => anyhow::bail!("unknown order status in database: {other}"),
    })
}

fn leg_status(s: &str) -> anyhow::Result<LegStatus> {
    Ok(match s {
        "pending"   => LegStatus::Pending,
        "picked_up" => LegStatus::PickedUp,
        "failed"    => LegStatus::Failed,
        "settled"   => LegStatus::Settled,
        other => anyhow::bail!("unknown leg status in database: {other}"),
    })
}

#[async_trait]
impl OrderRepository for PgOrderRepository {
    async fn save(&self, o: &Order) -> anyhow::Result<()> {
        // One transaction: an order without its legs is an unsettleable order,
        // and legs without their order are orphaned money.
        let mut tx = self.pool.begin().await?;

        sqlx::query(
            r#"
            INSERT INTO omnideliv.orders (
                id, tenant_id, customer_id, basket_id, plan_id, status,
                goods_total_cents, delivery_fee_cents, tip_cents, grand_total_cents,
                courier_trip_cents, courier_task_id, placed_at, delivered_at
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)
            ON CONFLICT (id) DO UPDATE SET
                status          = EXCLUDED.status,
                courier_task_id = EXCLUDED.courier_task_id,
                delivered_at    = EXCLUDED.delivered_at
            "#,
        )
        .bind(o.id).bind(o.tenant_id).bind(o.customer_id).bind(o.basket_id).bind(o.plan_id)
        .bind(o.status.as_str())
        .bind(o.goods_total_cents).bind(o.delivery_fee_cents).bind(o.tip_cents)
        .bind(o.grand_total_cents).bind(o.courier_trip_cents)
        .bind(o.courier_task_id).bind(o.placed_at).bind(o.delivered_at)
        .execute(&mut *tx).await?;

        for l in &o.legs {
            sqlx::query(
                r#"
                INSERT INTO omnideliv.order_vendor_legs (
                    id, order_id, tenant_id, vendor_id, goods_subtotal_cents,
                    commission_bps, commission_cents, payout_cents, status,
                    picked_up_at, created_at
                ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
                ON CONFLICT (id) DO UPDATE SET
                    status       = EXCLUDED.status,
                    picked_up_at = EXCLUDED.picked_up_at
                "#,
            )
            .bind(l.id).bind(l.order_id).bind(l.tenant_id).bind(l.vendor_id)
            .bind(l.goods_subtotal_cents).bind(l.commission_bps)
            .bind(l.commission_cents).bind(l.payout_cents)
            .bind(l.status.as_str()).bind(l.picked_up_at).bind(l.created_at)
            .execute(&mut *tx).await?;
        }

        tx.commit().await?;
        Ok(())
    }

    async fn find_by_id(&self, tenant_id: Uuid, id: Uuid) -> anyhow::Result<Option<Order>> {
        let Some(r) = sqlx::query("SELECT * FROM omnideliv.orders WHERE tenant_id = $1 AND id = $2")
            .bind(tenant_id).bind(id)
            .fetch_optional(&self.pool).await?
        else {
            return Ok(None);
        };

        let leg_rows = sqlx::query(
            "SELECT * FROM omnideliv.order_vendor_legs WHERE order_id = $1 ORDER BY created_at, id",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await?;

        let mut legs = Vec::with_capacity(leg_rows.len());
        for lr in &leg_rows {
            let st: String = lr.get("status");
            legs.push(VendorLeg {
                id:                   lr.get("id"),
                order_id:             lr.get("order_id"),
                tenant_id:            lr.get("tenant_id"),
                vendor_id:            lr.get("vendor_id"),
                goods_subtotal_cents: lr.get("goods_subtotal_cents"),
                commission_bps:       lr.get("commission_bps"),
                commission_cents:     lr.get("commission_cents"),
                payout_cents:         lr.get("payout_cents"),
                status:               leg_status(&st)?,
                picked_up_at:         lr.get("picked_up_at"),
                created_at:           lr.get("created_at"),
            });
        }

        let status: String = r.get("status");
        Ok(Some(Order {
            id:                 r.get("id"),
            tenant_id:          r.get("tenant_id"),
            customer_id:        r.get("customer_id"),
            basket_id:          r.get("basket_id"),
            plan_id:            r.get("plan_id"),
            status:             order_status(&status)?,
            goods_total_cents:  r.get("goods_total_cents"),
            delivery_fee_cents: r.get("delivery_fee_cents"),
            tip_cents:          r.get("tip_cents"),
            grand_total_cents:  r.get("grand_total_cents"),
            courier_trip_cents: r.get("courier_trip_cents"),
            courier_task_id:    r.get("courier_task_id"),
            legs,
            placed_at:          r.get("placed_at"),
            delivered_at:       r.get("delivered_at"),
        }))
    }
}
