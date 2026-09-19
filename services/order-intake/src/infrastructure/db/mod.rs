/// The whole-home moving catalogue.
pub mod home_catalogue;
/// Booked whole-home moves.
pub mod home_moves;
/// The survey's addendum to a booked home move.
pub mod home_addenda;
pub mod home_waitlist;

use std::pin::Pin;
use std::future::Future;

use sqlx::{PgPool, Row};
use uuid::Uuid;

use logisticos_types::{
    Address, Coordinates, Currency, CustomerId, MerchantId, Money,
    PieceStatus, ShipmentId, ShipmentStatus, TenantId,
    awb::Awb,
};

use crate::{
    application::services::shipment_service::{NewShipmentEvent, ShipmentListFilter, ShipmentRepository},
    domain::{
        entities::{shipment::{PaymentRequirement, Shipment}, piece::Piece},
        value_objects::{ServiceType, ShipmentDimensions, ShipmentWeight},
    },
};

pub struct PgShipmentRepository {
    pub pool: PgPool,
}

// Flat row returned from PostgreSQL
struct ShipmentRow {
    id:                   Uuid,
    tenant_id:            Uuid,
    merchant_id:          Uuid,
    customer_id:          Uuid,
    customer_name:        String,
    customer_phone:       String,
    customer_email:       Option<String>,
    booked_by_customer:   bool,
    auto_dispatch:        bool,
    awb:                  String,
    piece_count:          i16,
    status:               String,
    service_type:         String,

    origin_line1:         String,
    origin_line2:         Option<String>,
    origin_barangay:      Option<String>,
    origin_city:          String,
    origin_province:      String,
    origin_postal_code:   String,
    origin_country_code:  String,
    origin_lat:           Option<f64>,
    origin_lng:           Option<f64>,

    dest_line1:           String,
    dest_line2:           Option<String>,
    dest_barangay:        Option<String>,
    dest_city:            String,
    dest_province:        String,
    dest_postal_code:     String,
    dest_country_code:    String,
    dest_lat:             Option<f64>,
    dest_lng:             Option<f64>,

    weight_grams:         i32,
    length_cm:            Option<i32>,
    width_cm:             Option<i32>,
    height_cm:            Option<i32>,
    declared_value_cents: Option<i64>,
    cod_amount_cents:     Option<i64>,
    special_instructions: Option<String>,
    merchant_reference:   Option<String>,
    source_platform:      Option<String>,
    external_order_id:    Option<String>,

    payment_intent_id:       Option<Uuid>,
    payment_status:          String,
    pending_dispatch_events: Option<serde_json::Value>,
    idempotency_key:         Option<String>,

    created_at:           chrono::DateTime<chrono::Utc>,
    updated_at:           chrono::DateTime<chrono::Utc>,

    scheduled_pickup_at:         Option<chrono::DateTime<chrono::Utc>>,
    cancellation_policy_version: Option<String>,
    booking_amount_cents:        Option<i64>,
    booking_currency:            Option<String>,
}

impl ShipmentRow {
    /// Fallible rather than panicking on an unrecognized `payment_status`.
    ///
    /// This decode path backs both HTTP-request handlers (`find_by_id`, `list`,
    /// `find_by_idempotency_key`) *and* the unattended payment-intent-expiry
    /// sweep (`find_awaiting_payment_older_than`, added alongside this method).
    /// A `.expect()` here would be fine for the request-handler callers — the
    /// panic surfaces as one clean 500 to one caller — but a rolling deploy
    /// where a migration widens the `payment_status` CHECK constraint ahead of
    /// this binary picking up the matching `PaymentRequirement` variant would
    /// panic *inside the background sweep* instead, silently killing that
    /// worker task rather than skipping/erroring on the one bad row. Since one
    /// shared function now feeds both call shapes, propagating a `Result`
    /// keeps the background path safe without weakening the request path (its
    /// callers already turn `Err` into a 500 via `AppError::Internal`).
    fn into_shipment(self) -> anyhow::Result<Shipment> {
        let origin = Address {
            line1:        self.origin_line1,
            line2:        self.origin_line2,
            barangay:     self.origin_barangay,
            city:         self.origin_city,
            province:     self.origin_province,
            postal_code:  self.origin_postal_code,
            country_code: self.origin_country_code,
            coordinates:  match (self.origin_lat, self.origin_lng) {
                (Some(lat), Some(lng)) => Some(Coordinates { lat, lng }),
                _ => None,
            },
        };
        let destination = Address {
            line1:        self.dest_line1,
            line2:        self.dest_line2,
            barangay:     self.dest_barangay,
            city:         self.dest_city,
            province:     self.dest_province,
            postal_code:  self.dest_postal_code,
            country_code: self.dest_country_code,
            coordinates:  match (self.dest_lat, self.dest_lng) {
                (Some(lat), Some(lng)) => Some(Coordinates { lat, lng }),
                _ => None,
            },
        };
        let service_type = match self.service_type.as_str() {
            "express"       => ServiceType::Express,
            "same_day"      => ServiceType::SameDay,
            "balikbayan"    => ServiceType::Balikbayan,
            "international" => ServiceType::International,
            "home_move"     => ServiceType::HomeMove,
            _               => ServiceType::Standard,
        };
        let status = match self.status.as_str() {
            "confirmed"          => ShipmentStatus::Confirmed,
            "pickup_assigned"    => ShipmentStatus::PickupAssigned,
            "picked_up"          => ShipmentStatus::PickedUp,
            "in_transit"         => ShipmentStatus::InTransit,
            "at_hub"             => ShipmentStatus::AtHub,
            "out_for_delivery"   => ShipmentStatus::OutForDelivery,
            "delivery_attempted" => ShipmentStatus::DeliveryAttempted,
            "delivered"          => ShipmentStatus::Delivered,
            "partial_delivery"   => ShipmentStatus::PartialDelivery,
            "piece_exception"    => ShipmentStatus::PieceException,
            "customs_hold"       => ShipmentStatus::CustomsHold,
            "failed"             => ShipmentStatus::Failed,
            "cancelled"          => ShipmentStatus::Cancelled,
            "returned"           => ShipmentStatus::Returned,
            _                    => ShipmentStatus::Pending,
        };
        // Parse AWB — fall back to a raw string representation if malformed
        let awb = Awb::parse(&self.awb).unwrap_or_else(|_| {
            // For legacy or test rows that don't follow the new format
            let tenant = logisticos_types::awb::TenantCode::new("PH1").unwrap();
            Awb::generate(&tenant, logisticos_types::awb::ServiceCode::Standard, 1)
        });
        let payment_status = PaymentRequirement::parse(&self.payment_status).ok_or_else(|| {
            anyhow::anyhow!(
                "shipment {} has unrecognized payment_status value: {:?}",
                self.id,
                self.payment_status
            )
        })?;
        Ok(Shipment {
            id:                   ShipmentId::from_uuid(self.id),
            tenant_id:            TenantId::from_uuid(self.tenant_id),
            merchant_id:          MerchantId::from_uuid(self.merchant_id),
            customer_id:          CustomerId::from_uuid(self.customer_id),
            customer_name:        self.customer_name,
            customer_phone:       self.customer_phone,
            customer_email:       self.customer_email,
            booked_by_customer:   self.booked_by_customer,
            auto_dispatch:        self.auto_dispatch,
            awb,
            piece_count:          self.piece_count as u16,
            status,
            service_type,
            origin,
            destination,
            weight:               ShipmentWeight::from_grams(self.weight_grams as u32),
            dimensions:           match (self.length_cm, self.width_cm, self.height_cm) {
                (Some(l), Some(w), Some(h)) => Some(ShipmentDimensions {
                    length_cm: l as u32, width_cm: w as u32, height_cm: h as u32,
                }),
                _ => None,
            },
            declared_value:       self.declared_value_cents.map(|v| Money::new(v, Currency::PHP)),
            cod_amount:           self.cod_amount_cents.map(|v| Money::new(v, Currency::PHP)),
            special_instructions: self.special_instructions,
            merchant_reference:   self.merchant_reference,
            source_platform:      self.source_platform,
            external_order_id:    self.external_order_id,
            payment_intent_id:       self.payment_intent_id,
            payment_status,
            pending_dispatch_events: self.pending_dispatch_events,
            idempotency_key:         self.idempotency_key,
            created_at:           self.created_at,
            updated_at:           self.updated_at,
            scheduled_pickup_at:         self.scheduled_pickup_at,
            cancellation_policy_version: self.cancellation_policy_version,
            booking_amount_cents:        self.booking_amount_cents,
            booking_currency:            self.booking_currency,
        })
    }
}

fn status_str(s: &ShipmentStatus) -> &'static str {
    match s {
        ShipmentStatus::Pending           => "pending",
        ShipmentStatus::Confirmed         => "confirmed",
        ShipmentStatus::PickupAssigned    => "pickup_assigned",
        ShipmentStatus::PickedUp          => "picked_up",
        ShipmentStatus::InTransit         => "in_transit",
        ShipmentStatus::AtHub             => "at_hub",
        ShipmentStatus::OutForDelivery    => "out_for_delivery",
        ShipmentStatus::DeliveryAttempted => "delivery_attempted",
        ShipmentStatus::Delivered         => "delivered",
        ShipmentStatus::PartialDelivery   => "partial_delivery",
        ShipmentStatus::PieceException    => "piece_exception",
        ShipmentStatus::CustomsHold       => "customs_hold",
        ShipmentStatus::Failed            => "failed",
        ShipmentStatus::Cancelled         => "cancelled",
        ShipmentStatus::Returned          => "returned",
    }
}

const SHIPMENT_COLS: &str = r#"
    id, tenant_id, merchant_id, customer_id,
    customer_name, customer_phone, customer_email, booked_by_customer, auto_dispatch,
    awb, piece_count, status, service_type,
    origin_line1, origin_line2, origin_barangay, origin_city, origin_province,
    origin_postal_code, origin_country_code, origin_lat, origin_lng,
    dest_line1, dest_line2, dest_barangay, dest_city, dest_province,
    dest_postal_code, dest_country_code, dest_lat, dest_lng,
    weight_grams, length_cm, width_cm, height_cm,
    declared_value_cents, cod_amount_cents, special_instructions,
    merchant_reference, source_platform, external_order_id,
    payment_intent_id, payment_status, pending_dispatch_events, idempotency_key,
    created_at, updated_at,
    scheduled_pickup_at, cancellation_policy_version, booking_amount_cents, booking_currency
"#;

/// Maps a dynamic `PgRow` into the typed `ShipmentRow` struct.
fn row_to_shipment_row(r: &sqlx::postgres::PgRow) -> ShipmentRow {
    ShipmentRow {
        id:                   r.get("id"),
        tenant_id:            r.get("tenant_id"),
        merchant_id:          r.get("merchant_id"),
        customer_id:          r.get("customer_id"),
        customer_name:        r.get("customer_name"),
        customer_phone:       r.get("customer_phone"),
        customer_email:       r.get("customer_email"),
        booked_by_customer:   r.get("booked_by_customer"),
        auto_dispatch:        r.get("auto_dispatch"),
        awb:                  r.get("awb"),
        piece_count:          r.get("piece_count"),
        status:               r.get("status"),
        service_type:         r.get("service_type"),
        origin_line1:         r.get("origin_line1"),
        origin_line2:         r.get("origin_line2"),
        origin_barangay:      r.get("origin_barangay"),
        origin_city:          r.get("origin_city"),
        origin_province:      r.get("origin_province"),
        origin_postal_code:   r.get("origin_postal_code"),
        origin_country_code:  r.get("origin_country_code"),
        origin_lat:           r.get("origin_lat"),
        origin_lng:           r.get("origin_lng"),
        dest_line1:           r.get("dest_line1"),
        dest_line2:           r.get("dest_line2"),
        dest_barangay:        r.get("dest_barangay"),
        dest_city:            r.get("dest_city"),
        dest_province:        r.get("dest_province"),
        dest_postal_code:     r.get("dest_postal_code"),
        dest_country_code:    r.get("dest_country_code"),
        dest_lat:             r.get("dest_lat"),
        dest_lng:             r.get("dest_lng"),
        weight_grams:         r.get("weight_grams"),
        length_cm:            r.get("length_cm"),
        width_cm:             r.get("width_cm"),
        height_cm:            r.get("height_cm"),
        declared_value_cents: r.get("declared_value_cents"),
        cod_amount_cents:     r.get("cod_amount_cents"),
        special_instructions: r.get("special_instructions"),
        merchant_reference:   r.get("merchant_reference"),
        source_platform:      r.get("source_platform"),
        external_order_id:    r.get("external_order_id"),
        payment_intent_id:       r.get("payment_intent_id"),
        payment_status:          r.get("payment_status"),
        pending_dispatch_events: r.get("pending_dispatch_events"),
        idempotency_key:         r.get("idempotency_key"),
        created_at:           r.get("created_at"),
        updated_at:           r.get("updated_at"),
        scheduled_pickup_at:         r.get("scheduled_pickup_at"),
        cancellation_policy_version: r.get("cancellation_policy_version"),
        booking_amount_cents:        r.get("booking_amount_cents"),
        booking_currency:            r.get("booking_currency"),
    }
}

impl ShipmentRepository for PgShipmentRepository {
    fn list<'a>(
        &'a self,
        filter: &'a ShipmentListFilter,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<(Vec<Shipment>, i64)>> + Send + 'a>> {
        Box::pin(async move {
            let total: i64 = sqlx::query_scalar(
                r#"SELECT COUNT(*) FROM order_intake.shipments
                   WHERE tenant_id = $1
                     AND ($2::uuid IS NULL OR merchant_id = $2)
                     AND ($3::text IS NULL OR status = $3)
                     AND ($4::timestamptz IS NULL OR updated_at >= $4)
                     AND ($5::timestamptz IS NULL OR updated_at < $5)"#,
            )
            .bind(filter.tenant_id)
            .bind(filter.merchant_id)
            .bind(filter.status.as_deref())
            .bind(filter.updated_from)
            .bind(filter.updated_to)
            .fetch_one(&self.pool)
            .await?;

            let query = format!(
                r#"SELECT {} FROM order_intake.shipments
                   WHERE tenant_id = $1
                     AND ($2::uuid IS NULL OR merchant_id = $2)
                     AND ($3::text IS NULL OR status = $3)
                     AND ($4::timestamptz IS NULL OR updated_at >= $4)
                     AND ($5::timestamptz IS NULL OR updated_at < $5)
                   ORDER BY created_at DESC
                   LIMIT $6 OFFSET $7"#,
                SHIPMENT_COLS,
            );

            let rows = sqlx::query(&query)
                .bind(filter.tenant_id)
                .bind(filter.merchant_id)
                .bind(filter.status.as_deref())
                .bind(filter.updated_from)
                .bind(filter.updated_to)
                .bind(filter.limit)
                .bind(filter.offset)
                .fetch_all(&self.pool)
                .await?;

            let shipments = rows
                .into_iter()
                .map(|r| row_to_shipment_row(&r).into_shipment())
                .collect::<anyhow::Result<Vec<_>>>()?;

            Ok((shipments, total))
        })
    }

    fn find_by_id<'a>(
        &'a self,
        id: &'a ShipmentId,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Option<Shipment>>> + Send + 'a>> {
        Box::pin(async move {
            let query = format!(
                "SELECT {} FROM order_intake.shipments WHERE id = $1",
                SHIPMENT_COLS,
            );
            let row = sqlx::query(&query)
                .bind(id.inner())
                .fetch_optional(&self.pool)
                .await?;
            row.map(|r| row_to_shipment_row(&r).into_shipment()).transpose()
        })
    }

    fn save<'a>(
        &'a self,
        s: &'a Shipment,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let status = status_str(&s.status);
            let service_type = s.service_type.as_str();

            sqlx::query(
                r#"INSERT INTO order_intake.shipments (
                    id, tenant_id, merchant_id, customer_id,
                    customer_name, customer_phone, customer_email, booked_by_customer, auto_dispatch,
                    awb, piece_count, status, service_type,
                    origin_line1, origin_line2, origin_barangay, origin_city, origin_province,
                    origin_postal_code, origin_country_code, origin_lat, origin_lng,
                    dest_line1, dest_line2, dest_barangay, dest_city, dest_province,
                    dest_postal_code, dest_country_code, dest_lat, dest_lng,
                    weight_grams, length_cm, width_cm, height_cm,
                    declared_value_cents, cod_amount_cents, special_instructions,
                    merchant_reference, source_platform, external_order_id,
                    payment_intent_id, payment_status, pending_dispatch_events, idempotency_key,
                    created_at, updated_at,
                    scheduled_pickup_at, cancellation_policy_version, booking_amount_cents, booking_currency
                ) VALUES (
                    $1,$2,$3,$4,$5,$6,$7,$8,$9,
                    $10,$11,$12,$13,
                    $14,$15,$16,$17,$18,$19,$20,$21,$22,
                    $23,$24,$25,$26,$27,$28,$29,$30,$31,
                    $32,$33,$34,$35,$36,$37,$38,$39,$40,$41,
                    $42,$43,$44,$45,$46,$47,
                    $48,$49,$50,$51
                )
                ON CONFLICT (id) DO UPDATE SET
                    status               = EXCLUDED.status,
                    customer_name        = EXCLUDED.customer_name,
                    customer_phone       = EXCLUDED.customer_phone,
                    customer_email       = EXCLUDED.customer_email,
                    booked_by_customer   = EXCLUDED.booked_by_customer,
                    auto_dispatch        = EXCLUDED.auto_dispatch,
                    origin_lat           = EXCLUDED.origin_lat,
                    origin_lng           = EXCLUDED.origin_lng,
                    dest_lat             = EXCLUDED.dest_lat,
                    dest_lng             = EXCLUDED.dest_lng,
                    special_instructions = EXCLUDED.special_instructions,
                    merchant_reference   = EXCLUDED.merchant_reference,
                    source_platform      = EXCLUDED.source_platform,
                    external_order_id    = EXCLUDED.external_order_id,
                    payment_intent_id       = EXCLUDED.payment_intent_id,
                    payment_status          = EXCLUDED.payment_status,
                    pending_dispatch_events = EXCLUDED.pending_dispatch_events,
                    idempotency_key         = EXCLUDED.idempotency_key,
                    scheduled_pickup_at  = EXCLUDED.scheduled_pickup_at,
                    -- The version and the quoted total are fixed at booking.
                    cancellation_policy_version = COALESCE(order_intake.shipments.cancellation_policy_version, EXCLUDED.cancellation_policy_version),
                    booking_amount_cents = COALESCE(order_intake.shipments.booking_amount_cents, EXCLUDED.booking_amount_cents),
                    booking_currency     = COALESCE(order_intake.shipments.booking_currency, EXCLUDED.booking_currency),
                    updated_at           = EXCLUDED.updated_at"#,
            )
            .bind(s.id.inner())
            .bind(s.tenant_id.inner())
            .bind(s.merchant_id.inner())
            .bind(s.customer_id.inner())
            .bind(&s.customer_name)
            .bind(&s.customer_phone)
            .bind(s.customer_email.as_deref())
            .bind(s.booked_by_customer)
            .bind(s.auto_dispatch)
            .bind(s.awb.as_str())
            .bind(s.piece_count as i16)
            .bind(status)
            .bind(service_type)
            // origin
            .bind(&s.origin.line1)
            .bind(s.origin.line2.as_deref())
            .bind(s.origin.barangay.as_deref())
            .bind(&s.origin.city)
            .bind(&s.origin.province)
            .bind(&s.origin.postal_code)
            .bind(&s.origin.country_code)
            .bind(s.origin.coordinates.map(|c| c.lat))
            .bind(s.origin.coordinates.map(|c| c.lng))
            // destination
            .bind(&s.destination.line1)
            .bind(s.destination.line2.as_deref())
            .bind(s.destination.barangay.as_deref())
            .bind(&s.destination.city)
            .bind(&s.destination.province)
            .bind(&s.destination.postal_code)
            .bind(&s.destination.country_code)
            .bind(s.destination.coordinates.map(|c| c.lat))
            .bind(s.destination.coordinates.map(|c| c.lng))
            // parcel
            .bind(s.weight.grams as i32)
            .bind(s.dimensions.map(|d| d.length_cm as i32))
            .bind(s.dimensions.map(|d| d.width_cm as i32))
            .bind(s.dimensions.map(|d| d.height_cm as i32))
            .bind(s.declared_value.map(|m| m.amount))
            .bind(s.cod_amount.map(|m| m.amount))
            .bind(s.special_instructions.as_deref())
            .bind(s.merchant_reference.as_deref())
            .bind(s.source_platform.as_deref())
            .bind(s.external_order_id.as_deref())
            .bind(s.payment_intent_id)
            .bind(s.payment_status.as_str())
            .bind(&s.pending_dispatch_events)
            .bind(s.idempotency_key.as_deref())
            .bind(s.created_at)
            .bind(s.updated_at)
            .bind(s.scheduled_pickup_at)
            .bind(s.cancellation_policy_version.as_deref())
            .bind(s.booking_amount_cents)
            .bind(s.booking_currency.as_deref())
            .execute(&self.pool)
            .await?;
            Ok(())
        })
    }

    fn find_by_idempotency_key<'a>(
        &'a self,
        tenant_id: Uuid,
        idempotency_key: &'a str,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Option<Shipment>>> + Send + 'a>> {
        Box::pin(async move {
            let query = format!(
                "SELECT {SHIPMENT_COLS} FROM order_intake.shipments WHERE tenant_id = $1 AND idempotency_key = $2"
            );
            let row = sqlx::query(&query)
                .bind(tenant_id)
                .bind(idempotency_key)
                .fetch_optional(&self.pool)
                .await?;
            row.map(|r| row_to_shipment_row(&r).into_shipment()).transpose()
        })
    }

    fn cancel_if_awaiting_payment<'a>(
        &'a self,
        shipment_id: Uuid,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<bool>> + Send + 'a>> {
        Box::pin(async move {
            // The whole condition is evaluated by Postgres as part of the
            // write, so a concurrent payment capture (which sets
            // payment_status = 'paid') makes this match zero rows rather than
            // being clobbered by a stale in-memory copy.
            let result = sqlx::query(
                "UPDATE order_intake.shipments                     SET status = 'cancelled', updated_at = NOW()                   WHERE id = $1                     AND payment_status = 'awaiting_payment'                     AND status IN ('pending', 'confirmed')",
            )
            .bind(shipment_id)
            .execute(&self.pool)
            .await?;
            Ok(result.rows_affected() > 0)
        })
    }

    fn find_awaiting_payment_older_than<'a>(
        &'a self,
        cutoff: chrono::DateTime<chrono::Utc>,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Vec<Shipment>>> + Send + 'a>> {
        Box::pin(async move {
            let query = format!(
                "SELECT {SHIPMENT_COLS} FROM order_intake.shipments \
                 WHERE payment_status = 'awaiting_payment' AND created_at < $1"
            );
            let rows = sqlx::query(&query).bind(cutoff).fetch_all(&self.pool).await?;
            rows.iter()
                .map(|r| row_to_shipment_row(r).into_shipment())
                .collect::<anyhow::Result<Vec<_>>>()
        })
    }

    fn record_event<'a>(
        &'a self,
        event: NewShipmentEvent,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + 'a>> {
        Box::pin(async move {
            sqlx::query(
                r#"INSERT INTO order_intake.shipment_events
                       (tenant_id, shipment_id, event_type, from_status, to_status, actor_type, metadata)
                   VALUES ($1, $2, $3, $4, $5, $6,
                           jsonb_build_object('location', $7::text))"#,
            )
            .bind(event.tenant_id)
            .bind(event.shipment_id.inner())
            .bind(&event.event_type)
            .bind(event.from_status.as_deref())
            .bind(&event.to_status)
            .bind(&event.actor_type)
            .bind(event.location.as_deref())
            .execute(&self.pool)
            .await?;
            Ok(())
        })
    }

    fn settle_home_addendum<'a>(
        &'a self,
        addendum_id: uuid::Uuid,
        captured: bool,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Option<crate::application::services::shipment_service::SettledAddendum>>> + Send + 'a>> {
        Box::pin(async move {
            let mut tx = self.pool.begin().await?;
            if !captured {
                sqlx::query(
                    "UPDATE order_intake.home_addenda
                        SET status = 'pending', payment_intent_id = NULL, checkout_url = NULL, decided_at = NULL
                      WHERE id = $1 AND status = 'approved'",
                )
                .bind(addendum_id)
                .execute(&mut *tx)
                .await?;
                tx.commit().await?;
                return Ok(None);
            }
            // The trucks before, recorded on the addendum: more after it is a
            // major overflow, and an extra truck.
            let paid = sqlx::query(
                "UPDATE order_intake.home_addenda a SET status = 'paid', paid_at = NOW(),
                        trucks_before = (SELECT m.trucks FROM order_intake.home_moves m WHERE m.shipment_id = a.shipment_id)
                  WHERE a.id = $1 AND a.status = 'approved'
                  RETURNING a.shipment_id, a.tenant_id, a.items, a.total_cents, a.trucks, a.helpers, a.crew_total,
                            a.large_estate, a.trucks_before",
            )
            .bind(addendum_id)
            .fetch_optional(&mut *tx)
            .await?;
            let mut settled = None;
            // Already paid (a redelivery), or never approved: nothing to apply.
            if let Some(r) = paid {
                let shipment_id: uuid::Uuid = r.get("shipment_id");
                settled = Some(crate::application::services::shipment_service::SettledAddendum {
                    tenant_id: r.get("tenant_id"),
                    shipment_id,
                    addendum_id,
                    total_cents: r.get("total_cents"),
                    trucks_before: r.get::<Option<i32>, _>("trucks_before").unwrap_or(0),
                    trucks_after: r.get("trucks"),
                });
                sqlx::query(
                    "UPDATE order_intake.home_moves
                        SET items = items || $2, total_cents = total_cents + $3,
                            trucks = $4, helpers = $5, crew_total = $6, large_estate = $7
                      WHERE shipment_id = $1",
                )
                .bind(shipment_id)
                .bind(r.get::<serde_json::Value, _>("items"))
                .bind(r.get::<i64, _>("total_cents"))
                .bind(r.get::<i32, _>("trucks"))
                .bind(r.get::<i32, _>("helpers"))
                .bind(r.get::<i32, _>("crew_total"))
                .bind(r.get::<bool, _>("large_estate"))
                .execute(&mut *tx)
                .await?;
            }
            tx.commit().await?;
            Ok(settled)
        })
    }

    fn home_survey<'a>(
        &'a self,
        shipment_id: uuid::Uuid,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Option<crate::domain::value_objects::home_move::HomeSurvey>>> + Send + 'a>> {
        Box::pin(async move {
            type SurveyRow = (Option<chrono::DateTime<chrono::Utc>>, i64, Option<chrono::DateTime<chrono::Utc>>);
            let row: Option<SurveyRow> = sqlx::query_as(
                "SELECT survey_at, survey_cents, survey_submitted_at FROM order_intake.home_moves WHERE shipment_id = $1",
            )
            .bind(shipment_id)
            .fetch_optional(&self.pool)
            .await?;
            Ok(row.map(|(survey_at, survey_cents, submitted_at)| crate::domain::value_objects::home_move::HomeSurvey {
                survey_at,
                survey_cents,
                submitted: submitted_at.is_some(),
            }))
        })
    }

    fn record_intake<'a>(
        &'a self,
        tenant_id: uuid::Uuid,
        shipment_id: uuid::Uuid,
        intake: (String, Option<String>, Option<f64>, serde_json::Value),
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let (source, intent, confidence, extracted) = intake;
            sqlx::query(
                r#"INSERT INTO order_intake.shipment_intake
                       (shipment_id, tenant_id, source, intent, confidence, extracted)
                   VALUES ($1, $2, $3, $4, $5, $6)
                   ON CONFLICT (shipment_id) DO NOTHING"#,
            )
            .bind(shipment_id)
            .bind(tenant_id)
            .bind(source)
            .bind(intent)
            .bind(confidence)
            .bind(extracted)
            .execute(&self.pool)
            .await?;
            Ok(())
        })
    }

    fn save_pieces<'a>(
        &'a self,
        pieces: &'a [Piece],
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + 'a>> {
        Box::pin(async move {
            for p in pieces {
                let status = match p.status {
                    PieceStatus::Pending    => "pending",
                    PieceStatus::ScannedIn  => "at_hub",
                    PieceStatus::InTransit  => "in_transit",
                    PieceStatus::ScannedOut => "out_for_delivery",
                    PieceStatus::Delivered  => "delivered",
                    PieceStatus::Missing    => "exception",
                    PieceStatus::Damaged    => "exception",
                };
                sqlx::query(
                    r#"INSERT INTO order_intake.shipment_pieces (
                        id, shipment_id, tenant_id, piece_number, piece_awb,
                        declared_weight_g,
                        length_cm, width_cm, height_cm,
                        description, status,
                        created_at, updated_at
                    )
                    SELECT $1, $2,
                           (SELECT tenant_id FROM order_intake.shipments WHERE id = $2),
                           $3, $4, $5, $6, $7, $8, $9, $10, $11, $12
                    ON CONFLICT (id) DO UPDATE SET
                        status     = EXCLUDED.status,
                        updated_at = EXCLUDED.updated_at"#,
                )
                .bind(p.id)
                .bind(p.shipment_id.inner())
                .bind(p.piece_number as i16)
                .bind(p.piece_awb.as_str())
                .bind(p.declared_weight.grams as i32)
                .bind(p.dimensions.map(|d| d.length_cm as i32))
                .bind(p.dimensions.map(|d| d.width_cm as i32))
                .bind(p.dimensions.map(|d| d.height_cm as i32))
                .bind(p.description.as_deref())
                .bind(status)
                .bind(p.created_at)
                .bind(p.updated_at)
                .execute(&self.pool)
                .await?;
            }
            Ok(())
        })
    }
}
