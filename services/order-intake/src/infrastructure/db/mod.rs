use std::pin::Pin;
use std::future::Future;

use sqlx::PgPool;
use uuid::Uuid;
use serde_json;

use logisticos_types::{
    Address, Coordinates, Currency, CustomerId, MerchantId, Money, ShipmentId, ShipmentStatus, TenantId,
};

use crate::{
    application::services::shipment_service::{ShipmentListFilter, ShipmentRepository},
    domain::{
        entities::shipment::Shipment,
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
    tracking_number:      String,
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
            "failed"             => ShipmentStatus::Failed,
            "cancelled"          => ShipmentStatus::Cancelled,
            "returned"           => ShipmentStatus::Returned,
            _                    => ShipmentStatus::Pending,
        };
        Shipment {
            id:                   ShipmentId::from_uuid(self.id),
            tenant_id:            TenantId::from_uuid(self.tenant_id),
            merchant_id:          MerchantId::from_uuid(self.merchant_id),
            customer_id:          CustomerId::from_uuid(self.customer_id),
            tracking_number:      self.tracking_number,
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
        ShipmentStatus::Failed            => "failed",
        ShipmentStatus::Cancelled         => "cancelled",
        ShipmentStatus::Returned          => "returned",
    }
}

impl ShipmentRepository for PgShipmentRepository {
    fn list<'a>(
        &'a self,
        filter: &'a ShipmentListFilter,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<(Vec<Shipment>, i64)>> + Send + 'a>> {
        Box::pin(async move {
            // Count query
            let total: i64 = sqlx::query_scalar!(
                r#"SELECT COUNT(*) FROM order_intake.shipments
                   WHERE tenant_id = $1
                     AND ($2::uuid IS NULL OR merchant_id = $2)
                     AND ($3::text IS NULL OR status = $3)"#,
                filter.tenant_id,
                filter.merchant_id,
                filter.status.as_deref(),
            )
            .fetch_one(&self.pool)
            .await?
            .unwrap_or(0);

            // Data query
            let rows = sqlx::query_as!(
                ShipmentRow,
                r#"SELECT id, tenant_id, merchant_id, customer_id, tracking_number, status, service_type,
                          origin_line1, origin_line2, origin_barangay, origin_city, origin_province,
                          origin_postal_code, origin_country_code, origin_lat, origin_lng,
                          dest_line1, dest_line2, dest_barangay, dest_city, dest_province,
                          dest_postal_code, dest_country_code, dest_lat, dest_lng,
                          weight_grams, length_cm, width_cm, height_cm,
                          declared_value_cents, cod_amount_cents, special_instructions,
                          created_at, updated_at
                   FROM order_intake.shipments
                   WHERE tenant_id = $1
                     AND ($2::uuid IS NULL OR merchant_id = $2)
                     AND ($3::text IS NULL OR status = $3)
                   ORDER BY created_at DESC
                   LIMIT $4 OFFSET $5"#,
                filter.tenant_id,
                filter.merchant_id,
                filter.status.as_deref(),
                filter.limit,
                filter.offset,
            )
            .fetch_all(&self.pool)
            .await?;

            Ok((rows.into_iter().map(|r| r.into_shipment()).collect(), total))
        })
    }

    fn find_by_id<'a>(
        &'a self,
        id: &'a ShipmentId,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Option<Shipment>>> + Send + 'a>> {
        Box::pin(async move {
            let row = sqlx::query_as!(
                ShipmentRow,
                r#"
                SELECT id, tenant_id, merchant_id, customer_id, tracking_number, status, service_type,
                       origin_line1, origin_line2, origin_barangay, origin_city, origin_province,
                       origin_postal_code, origin_country_code, origin_lat, origin_lng,
                       dest_line1, dest_line2, dest_barangay, dest_city, dest_province,
                       dest_postal_code, dest_country_code, dest_lat, dest_lng,
                       weight_grams, length_cm, width_cm, height_cm,
                       declared_value_cents, cod_amount_cents, special_instructions,
                       created_at, updated_at
                FROM order_intake.shipments
                WHERE id = $1
                "#,
                id.inner()
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row.map(|r| r.into_shipment()))
        })
    }

    fn save<'a>(
        &'a self,
        s: &'a Shipment,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let status = status_str(&s.status);
            let service_type = s.service_type.as_str();

            sqlx::query!(
                r#"
                INSERT INTO order_intake.shipments (
                    id, tenant_id, merchant_id, customer_id, tracking_number, status, service_type,
                    origin_line1, origin_line2, origin_barangay, origin_city, origin_province,
                    origin_postal_code, origin_country_code, origin_lat, origin_lng,
                    dest_line1, dest_line2, dest_barangay, dest_city, dest_province,
                    dest_postal_code, dest_country_code, dest_lat, dest_lng,
                    weight_grams, length_cm, width_cm, height_cm,
                    declared_value_cents, cod_amount_cents, special_instructions,
                    created_at, updated_at
                ) VALUES (
                    $1,$2,$3,$4,$5,$6,$7,
                    $8,$9,$10,$11,$12,$13,$14,$15,$16,
                    $17,$18,$19,$20,$21,$22,$23,$24,$25,
                    $26,$27,$28,$29,$30,$31,$32,$33,$34
                )
                ON CONFLICT (id) DO UPDATE SET
                    status               = EXCLUDED.status,
                    origin_lat           = EXCLUDED.origin_lat,
                    origin_lng           = EXCLUDED.origin_lng,
                    dest_lat             = EXCLUDED.dest_lat,
                    dest_lng             = EXCLUDED.dest_lng,
                    special_instructions = EXCLUDED.special_instructions,
                    updated_at           = EXCLUDED.updated_at
                "#,
                s.id.inner(),
                s.tenant_id.inner(),
                s.merchant_id.inner(),
                s.customer_id.inner(),
                s.tracking_number,
                status,
                service_type,
                // origin
                s.origin.line1,
                s.origin.line2.as_deref(),
                s.origin.barangay.as_deref(),
                s.origin.city,
                s.origin.province,
                s.origin.postal_code,
                s.origin.country_code,
                s.origin.coordinates.map(|c| c.lat),
                s.origin.coordinates.map(|c| c.lng),
                // destination
                s.destination.line1,
                s.destination.line2.as_deref(),
                s.destination.barangay.as_deref(),
                s.destination.city,
                s.destination.province,
                s.destination.postal_code,
                s.destination.country_code,
                s.destination.coordinates.map(|c| c.lat),
                s.destination.coordinates.map(|c| c.lng),
                // parcel
                s.weight.grams as i32,
                s.dimensions.map(|d| d.length_cm as i32),
                s.dimensions.map(|d| d.width_cm as i32),
                s.dimensions.map(|d| d.height_cm as i32),
                s.declared_value.map(|m| m.amount),
                s.cod_amount.map(|m| m.amount),
                s.special_instructions.as_deref(),
                s.created_at,
                s.updated_at,
            )
            .execute(&self.pool)
            .await?;
            Ok(())
        })
    }
}
