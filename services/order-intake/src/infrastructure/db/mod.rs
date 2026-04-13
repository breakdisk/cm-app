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
    application::services::shipment_service::{ShipmentListFilter, ShipmentRepository},
    domain::{
        entities::{shipment::Shipment, piece::Piece},
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

    created_at:           chrono::DateTime<chrono::Utc>,
    updated_at:           chrono::DateTime<chrono::Utc>,
}

impl ShipmentRow {
    fn into_shipment(self) -> Shipment {
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
            "express"    => ServiceType::Express,
            "same_day"   => ServiceType::SameDay,
            "balikbayan" => ServiceType::Balikbayan,
            _            => ServiceType::Standard,
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
        Shipment {
            id:                   ShipmentId::from_uuid(self.id),
            tenant_id:            TenantId::from_uuid(self.tenant_id),
            merchant_id:          MerchantId::from_uuid(self.merchant_id),
            customer_id:          CustomerId::from_uuid(self.customer_id),
            customer_name:        self.customer_name,
            customer_phone:       self.customer_phone,
            customer_email:       self.customer_email,
            booked_by_customer:   self.booked_by_customer,
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
            created_at:           self.created_at,
            updated_at:           self.updated_at,
        }
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
    customer_name, customer_phone, customer_email, booked_by_customer,
    awb, piece_count, status, service_type,
    origin_line1, origin_line2, origin_barangay, origin_city, origin_province,
    origin_postal_code, origin_country_code, origin_lat, origin_lng,
    dest_line1, dest_line2, dest_barangay, dest_city, dest_province,
    dest_postal_code, dest_country_code, dest_lat, dest_lng,
    weight_grams, length_cm, width_cm, height_cm,
    declared_value_cents, cod_amount_cents, special_instructions,
    created_at, updated_at
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
        created_at:           r.get("created_at"),
        updated_at:           r.get("updated_at"),
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
                     AND ($3::text IS NULL OR status = $3)"#,
            )
            .bind(filter.tenant_id)
            .bind(filter.merchant_id)
            .bind(filter.status.as_deref())
            .fetch_one(&self.pool)
            .await?;

            let query = format!(
                r#"SELECT {} FROM order_intake.shipments
                   WHERE tenant_id = $1
                     AND ($2::uuid IS NULL OR merchant_id = $2)
                     AND ($3::text IS NULL OR status = $3)
                   ORDER BY created_at DESC
                   LIMIT $4 OFFSET $5"#,
                SHIPMENT_COLS,
            );

            let rows = sqlx::query(&query)
                .bind(filter.tenant_id)
                .bind(filter.merchant_id)
                .bind(filter.status.as_deref())
                .bind(filter.limit)
                .bind(filter.offset)
                .fetch_all(&self.pool)
                .await?;

            let shipments = rows
                .into_iter()
                .map(|r| row_to_shipment_row(&r).into_shipment())
                .collect();

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
            Ok(row.map(|r| row_to_shipment_row(&r).into_shipment()))
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
                    customer_name, customer_phone, customer_email, booked_by_customer,
                    awb, piece_count, status, service_type,
                    origin_line1, origin_line2, origin_barangay, origin_city, origin_province,
                    origin_postal_code, origin_country_code, origin_lat, origin_lng,
                    dest_line1, dest_line2, dest_barangay, dest_city, dest_province,
                    dest_postal_code, dest_country_code, dest_lat, dest_lng,
                    weight_grams, length_cm, width_cm, height_cm,
                    declared_value_cents, cod_amount_cents, special_instructions,
                    created_at, updated_at
                ) VALUES (
                    $1,$2,$3,$4,$5,$6,$7,$8,
                    $9,$10,$11,$12,
                    $13,$14,$15,$16,$17,$18,$19,$20,$21,
                    $22,$23,$24,$25,$26,$27,$28,$29,$30,
                    $31,$32,$33,$34,$35,$36,$37,$38,$39
                )
                ON CONFLICT (id) DO UPDATE SET
                    status               = EXCLUDED.status,
                    customer_name        = EXCLUDED.customer_name,
                    customer_phone       = EXCLUDED.customer_phone,
                    customer_email       = EXCLUDED.customer_email,
                    booked_by_customer   = EXCLUDED.booked_by_customer,
                    origin_lat           = EXCLUDED.origin_lat,
                    origin_lng           = EXCLUDED.origin_lng,
                    dest_lat             = EXCLUDED.dest_lat,
                    dest_lng             = EXCLUDED.dest_lng,
                    special_instructions = EXCLUDED.special_instructions,
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
            .bind(s.created_at)
            .bind(s.updated_at)
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
