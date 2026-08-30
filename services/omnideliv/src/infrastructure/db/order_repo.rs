use async_trait::async_trait;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::domain::entities::{LegStatus, Order, OrderStatus, PaymentMethod, PaymentStatus, VendorLeg};
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

fn payment_method(s: &str) -> anyhow::Result<PaymentMethod> {
    Ok(match s {
        "cod"    => PaymentMethod::Cod,
        "online" => PaymentMethod::Online,
        other => anyhow::bail!("unknown payment method in database: {other}"),
    })
}

fn payment_status(s: &str) -> anyhow::Result<PaymentStatus> {
    Ok(match s {
        "pending"    => PaymentStatus::Pending,
        "authorized" => PaymentStatus::Authorized,
        "captured"   => PaymentStatus::Captured,
        "voided"     => PaymentStatus::Voided,
        "failed"     => PaymentStatus::Failed,
        other => anyhow::bail!("unknown payment status in database: {other}"),
    })
}

fn leg_status(s: &str) -> anyhow::Result<LegStatus> {
    LegStatus::from_wire(s).ok_or_else(|| anyhow::anyhow!("unknown leg status: {s}"))
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
                courier_trip_cents, courier_task_id, placed_at, delivered_at,
                delivery_lat, delivery_lng, customer_name, customer_phone, courier_user_id,
                delivery_note, payment_method, payment_status, payment_intent_id,
                prepaid_amount_cents, payment_authorized_at, pending_offer_card,
                payment_checkout_url
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,
                      $21,$22,$23,$24,$25,$26,$27)
            ON CONFLICT (id) DO UPDATE SET
                status          = EXCLUDED.status,
                courier_task_id = EXCLUDED.courier_task_id,
                delivered_at    = EXCLUDED.delivered_at,
                -- COALESCE, not a plain assignment: an order loaded from before
                -- migration 0013 carries NULL coordinates, and saving it after a
                -- status change must not erase a destination that was set since.
                delivery_lat    = COALESCE(EXCLUDED.delivery_lat, omnideliv.orders.delivery_lat),
                delivery_lng    = COALESCE(EXCLUDED.delivery_lng, omnideliv.orders.delivery_lng),
                -- Same reasoning, same trap: the recovery sweep and the courier
                -- consumer both save orders they loaded, and neither knows a
                -- contact. A plain assignment would erase the customer's phone
                -- on the first status change after checkout.
                customer_name   = COALESCE(EXCLUDED.customer_name, omnideliv.orders.customer_name),
                delivery_note   = COALESCE(EXCLUDED.delivery_note, omnideliv.orders.delivery_note),
                customer_phone  = COALESCE(EXCLUDED.customer_phone, omnideliv.orders.customer_phone),
                courier_user_id = COALESCE(EXCLUDED.courier_user_id, omnideliv.orders.courier_user_id),
                -- Mutable after creation — the payment.intent.authorized
                -- consumer, the courier-milestone capture, and the recovery
                -- sweep's void all advance these on an already-persisted order.
                -- `payment_method` and `prepaid_amount_cents` are deliberately
                -- absent from this list: both are fixed at checkout and never
                -- change again, exactly like `goods_total_cents` above them.
                payment_status         = EXCLUDED.payment_status,
                payment_intent_id      = EXCLUDED.payment_intent_id,
                payment_authorized_at  = EXCLUDED.payment_authorized_at
            "#,
        )
        .bind(o.id).bind(o.tenant_id).bind(o.customer_id).bind(o.basket_id).bind(o.plan_id)
        .bind(o.status.as_str())
        .bind(o.goods_total_cents).bind(o.delivery_fee_cents).bind(o.tip_cents)
        .bind(o.grand_total_cents).bind(o.courier_trip_cents)
        .bind(o.courier_task_id).bind(o.placed_at).bind(o.delivered_at)
        .bind(o.delivery_lat).bind(o.delivery_lng)
        .bind(&o.customer_name).bind(&o.customer_phone).bind(o.courier_user_id)
        .bind(&o.delivery_note)
        .bind(o.payment_method.as_str()).bind(o.payment_status.as_str())
        .bind(o.payment_intent_id).bind(o.prepaid_amount_cents).bind(o.payment_authorized_at)
        .bind(&o.pending_offer_card)
        .bind(&o.payment_checkout_url)
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

    async fn find_awaiting_courier(&self) -> anyhow::Result<Vec<Order>> {
        // Legs are not loaded: the sweep only reads status and placed_at, and
        // fetching every leg for every stuck order would make an operational
        // sweep proportional to basket size for no benefit.
        let rows = sqlx::query(
            "SELECT * FROM omnideliv.orders
              WHERE status IN ('placed', 'awaiting_courier')
              ORDER BY placed_at ASC
              LIMIT 500",
        )
        .fetch_all(&self.pool)
        .await?;

        let mut out = Vec::with_capacity(rows.len());
        for r in &rows {
            let status: String = r.get("status");
            let pm: String = r.get("payment_method");
            let ps: String = r.get("payment_status");
            out.push(Order {
                id:                 r.get("id"),
                tenant_id:          r.get("tenant_id"),
                customer_id:        r.get("customer_id"),
                basket_id:          r.get("basket_id"),
                plan_id:            r.get("plan_id"),
                status:             order_status(&status)?,
                delivery_lat:       r.get("delivery_lat"),
                delivery_lng:       r.get("delivery_lng"),
                courier_user_id:    r.get("courier_user_id"),
                customer_name:      r.get("customer_name"),
            delivery_note:      r.get("delivery_note"),
                customer_phone:     r.get("customer_phone"),
                goods_total_cents:  r.get("goods_total_cents"),
                delivery_fee_cents: r.get("delivery_fee_cents"),
                tip_cents:          r.get("tip_cents"),
                grand_total_cents:  r.get("grand_total_cents"),
                courier_trip_cents: r.get("courier_trip_cents"),
                courier_task_id:    r.get("courier_task_id"),
                legs:               Vec::new(),
                placed_at:          r.get("placed_at"),
                delivered_at:       r.get("delivered_at"),
                payment_method:        payment_method(&pm)?,
                payment_status:        payment_status(&ps)?,
                payment_intent_id:     r.get("payment_intent_id"),
                prepaid_amount_cents:  r.get("prepaid_amount_cents"),
                payment_authorized_at: r.get("payment_authorized_at"),
                pending_offer_card:    r.get("pending_offer_card"),
                payment_checkout_url:  r.get("payment_checkout_url"),
            });
        }
        Ok(out)
    }

    async fn list_summaries_for_customer(
        &self,
        tenant_id:   Uuid,
        customer_id: Uuid,
        limit:       i64,
    ) -> anyhow::Result<Vec<crate::domain::repositories::OrderSummary>> {
        // Every column is qualified. `orders`, `order_vendor_legs` and
        // `vendors` all carry tenant_id/created_at, and an unqualified name
        // across a three-way join is rejected outright as ambiguous.
        //
        // LEFT JOINs so an order with no legs still appears — a broken order is
        // exactly the one a customer needs to see.
        let rows = sqlx::query(
            r#"
            SELECT o.id                AS id,
                   o.status            AS status,
                   o.goods_total_cents  AS goods_total_cents,
                   o.delivery_fee_cents AS delivery_fee_cents,
                   o.tip_cents          AS tip_cents,
                   o.grand_total_cents AS grand_total_cents,
                   o.payment_method    AS payment_method,
                   o.payment_status    AS payment_status,
                   o.prepaid_amount_cents AS prepaid_amount_cents,
                   o.placed_at         AS placed_at,
                   o.delivered_at      AS delivered_at,
                   COUNT(l.id)         AS stops_total,
                   COALESCE(STRING_AGG(DISTINCT v.name, ', '), '') AS vendor_names
              FROM omnideliv.orders o
              LEFT JOIN omnideliv.order_vendor_legs l ON l.order_id  = o.id
              LEFT JOIN omnideliv.vendors          v ON v.id        = l.vendor_id
             WHERE o.tenant_id = $1 AND o.customer_id = $2
             GROUP BY o.id
             ORDER BY o.placed_at DESC
             LIMIT $3
            "#,
        )
        .bind(tenant_id)
        .bind(customer_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .iter()
            .map(|r| crate::domain::repositories::OrderSummary {
                id:                r.get("id"),
                status:            r.get("status"),
                goods_total_cents:  r.get("goods_total_cents"),
                delivery_fee_cents: r.get("delivery_fee_cents"),
                tip_cents:          r.get("tip_cents"),
                grand_total_cents: r.get("grand_total_cents"),
                payment_method:    r.get("payment_method"),
                payment_status:    r.get("payment_status"),
                prepaid_amount_cents: r.get("prepaid_amount_cents"),
                stops_total:       r.get("stops_total"),
                vendor_names:      r.get("vendor_names"),
                placed_at:         r.get("placed_at"),
                delivered_at:      r.get("delivered_at"),
            })
            .collect())
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
        let pm: String = r.get("payment_method");
        let ps: String = r.get("payment_status");
        Ok(Some(Order {
            id:                 r.get("id"),
            tenant_id:          r.get("tenant_id"),
            customer_id:        r.get("customer_id"),
            basket_id:          r.get("basket_id"),
            plan_id:            r.get("plan_id"),
            status:             order_status(&status)?,
            delivery_lat:       r.get("delivery_lat"),
            delivery_lng:       r.get("delivery_lng"),
            courier_user_id:    r.get("courier_user_id"),
            customer_name:      r.get("customer_name"),
            delivery_note:      r.get("delivery_note"),
            customer_phone:     r.get("customer_phone"),
            goods_total_cents:  r.get("goods_total_cents"),
            delivery_fee_cents: r.get("delivery_fee_cents"),
            tip_cents:          r.get("tip_cents"),
            grand_total_cents:  r.get("grand_total_cents"),
            courier_trip_cents: r.get("courier_trip_cents"),
            courier_task_id:    r.get("courier_task_id"),
            legs,
            placed_at:          r.get("placed_at"),
            delivered_at:       r.get("delivered_at"),
            payment_method:        payment_method(&pm)?,
            payment_status:        payment_status(&ps)?,
            payment_intent_id:     r.get("payment_intent_id"),
            prepaid_amount_cents:  r.get("prepaid_amount_cents"),
            payment_authorized_at: r.get("payment_authorized_at"),
            pending_offer_card:    r.get("pending_offer_card"),
            payment_checkout_url:  r.get("payment_checkout_url"),
        }))
    }
}
