//! ShipmentService — orchestrates order intake business logic.

use std::sync::Arc;
use chrono::Utc;
use logisticos_events::{Event, payloads::ShipmentCreated, topics};
use logisticos_errors::{AppError, AppResult};
use logisticos_types::{ShipmentId, MerchantId, CustomerId, Money, Currency, ShipmentStatus, TenantId};
use crate::{
    application::commands::{
        CreateShipmentCommand, CancelShipmentCommand,
        BulkCreateShipmentCommand, BulkCreateResult, BulkRowError,
    },
    domain::{
        entities::shipment::Shipment,
        value_objects::{ServiceType, ShipmentWeight, ShipmentDimensions, TrackingNumber},
    },
};

pub struct ShipmentListFilter {
    pub tenant_id:   uuid::Uuid,
    pub merchant_id: Option<uuid::Uuid>,
    pub status:      Option<String>,
    pub limit:       i64,
    pub offset:      i64,
}

pub trait ShipmentRepository: Send + Sync {
    fn find_by_id<'a>(
        &'a self,
        id: &'a ShipmentId,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<Option<Shipment>>> + Send + 'a>>;

    fn save<'a>(
        &'a self,
        shipment: &'a Shipment,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + 'a>>;

    fn list<'a>(
        &'a self,
        filter: &'a ShipmentListFilter,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<(Vec<Shipment>, i64)>> + Send + 'a>>;
}

pub trait EventPublisher: Send + Sync {
    fn publish<'a>(
        &'a self,
        topic: &'a str,
        key: &'a str,
        payload: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + 'a>>;
}

pub trait AddressNormalizer: Send + Sync {
    fn normalize<'a>(
        &'a self,
        input: &'a crate::application::commands::AddressInput,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<logisticos_types::Address>> + Send + 'a>>;
}

pub struct ShipmentService {
    pub repo:       Arc<dyn ShipmentRepository>,
    pub publisher:  Arc<dyn EventPublisher>,
    pub normalizer: Arc<dyn AddressNormalizer>,
}

impl ShipmentService {
    pub fn new(
        repo: Arc<dyn ShipmentRepository>,
        publisher: Arc<dyn EventPublisher>,
        normalizer: Arc<dyn AddressNormalizer>,
    ) -> Self {
        Self { repo, publisher, normalizer }
    }

    pub async fn create(&self, cmd: CreateShipmentCommand) -> AppResult<Shipment> {
        // Parse service type
        let service_type = match cmd.service_type.as_str() {
            "standard"   => ServiceType::Standard,
            "express"    => ServiceType::Express,
            "same_day"   => ServiceType::SameDay,
            "balikbayan" => ServiceType::Balikbayan,
            other => return Err(AppError::Validation(format!("Unknown service type: {other}"))),
        };

        // Business rule: same-day cutoff at 14:00
        if service_type == ServiceType::SameDay {
            let hour = Utc::now().format("%H").to_string().parse::<u32>().unwrap_or(0);
            if hour >= 14 {
                return Err(AppError::BusinessRule(
                    "Same-day orders must be placed before 14:00 local time".into(),
                ));
            }
        }

        // Validate weight
        let weight = ShipmentWeight::from_grams(cmd.weight_grams);
        weight.validate().map_err(|e| AppError::Validation(e.to_string()))?;

        // Business rule: COD must not exceed declared value
        if let (Some(cod), Some(declared)) = (cmd.cod_amount_cents, cmd.declared_value_cents) {
            if cod > declared {
                return Err(AppError::BusinessRule(
                    "COD amount cannot exceed declared value".into(),
                ));
            }
        }

        // Normalize and geocode addresses
        let origin      = self.normalizer.normalize(&cmd.origin).await.map_err(AppError::Internal)?;
        let destination = self.normalizer.normalize(&cmd.destination).await.map_err(AppError::Internal)?;

        // Billable weight = max(actual, volumetric)
        let dimensions = match (cmd.length_cm, cmd.width_cm, cmd.height_cm) {
            (Some(l), Some(w), Some(h)) => Some(ShipmentDimensions { length_cm: l, width_cm: w, height_cm: h }),
            _ => None,
        };
        let billable_grams = dimensions
            .map(|d| d.volumetric_weight_grams().max(weight.grams))
            .unwrap_or(weight.grams);

        let now = Utc::now();
        let shipment = Shipment {
            id: ShipmentId::new(),
            tenant_id: TenantId::from_uuid(cmd.tenant_id),
            merchant_id: MerchantId::from_uuid(cmd.merchant_id),
            customer_id: CustomerId::new(),
            tracking_number: TrackingNumber::generate(),
            status: ShipmentStatus::Pending,
            service_type,
            origin,
            destination,
            weight: ShipmentWeight::from_grams(billable_grams),
            dimensions,
            declared_value: cmd.declared_value_cents.map(|v| Money::new(v, Currency::PHP)),
            cod_amount: cmd.cod_amount_cents.map(|v| Money::new(v, Currency::PHP)),
            special_instructions: cmd.special_instructions,
            created_at: now,
            updated_at: now,
        };

        self.repo.save(&shipment).await.map_err(AppError::Internal)?;

        // Emit event — dispatch service subscribes
        let event = Event::new(
            "logisticos/order-intake",
            "shipment.created",
            cmd.tenant_id,
            ShipmentCreated {
                shipment_id:          shipment.id.inner(),
                merchant_id:          cmd.merchant_id,
                customer_id:          shipment.customer_id.inner(),
                customer_name:        cmd.customer_name.clone(),
                customer_phone:       cmd.customer_phone.clone(),
                origin_address:       format!("{}, {}", shipment.origin.city, shipment.origin.province),
                destination_address:  format!("{}, {}", shipment.destination.city, shipment.destination.province),
                destination_city:     shipment.destination.city.clone(),
                destination_lat:      shipment.destination.coordinates.map(|c| c.lat),
                destination_lng:      shipment.destination.coordinates.map(|c| c.lng),
                service_type:         service_type.as_str().into(),
                cod_amount_cents:     shipment.cod_amount.map(|m| m.amount),
            },
        );
        let payload = serde_json::to_string(&event).map_err(|e| AppError::Internal(e.into()))?;
        self.publisher
            .publish(topics::SHIPMENT_CREATED, &shipment.id.to_string(), &payload)
            .await
            .map_err(AppError::Internal)?;

        tracing::info!(
            shipment_id  = %shipment.id,
            tracking     = %shipment.tracking_number,
            service_type = service_type.as_str(),
            "Shipment created"
        );
        Ok(shipment)
    }

    pub async fn cancel(&self, cmd: CancelShipmentCommand) -> AppResult<()> {
        let id = ShipmentId::from_uuid(cmd.shipment_id);
        let mut shipment = self.repo.find_by_id(&id).await.map_err(AppError::Internal)?
            .ok_or(AppError::NotFound { resource: "Shipment", id: cmd.shipment_id.to_string() })?;

        if !shipment.can_cancel() {
            return Err(AppError::BusinessRule(
                format!("Cannot cancel shipment in status {:?}", shipment.status),
            ));
        }

        shipment.status   = ShipmentStatus::Cancelled;
        shipment.updated_at = Utc::now();
        self.repo.save(&shipment).await.map_err(AppError::Internal)?;

        let event = Event::new(
            "logisticos/order-intake",
            "shipment.cancelled",
            uuid::Uuid::nil(),
            serde_json::json!({ "shipment_id": shipment.id.inner(), "reason": cmd.reason }),
        );
        let payload = serde_json::to_string(&event).map_err(|e| AppError::Internal(e.into()))?;
        self.publisher
            .publish(topics::SHIPMENT_CANCELLED, &shipment.id.to_string(), &payload)
            .await
            .map_err(AppError::Internal)?;

        Ok(())
    }

    pub async fn bulk_create(&self, cmd: BulkCreateShipmentCommand) -> AppResult<BulkCreateResult> {
        let mut created = Vec::new();
        let mut failed  = Vec::new();

        for (i, row) in cmd.rows.into_iter().enumerate() {
            let reference = row.merchant_reference.clone();
            match self.create(row).await {
                Ok(s)  => created.push(s.id.inner()),
                Err(e) => failed.push(BulkRowError {
                    row_index: i,
                    merchant_reference: reference,
                    error: e.to_string(),
                }),
            }
        }

        tracing::info!(created = created.len(), failed = failed.len(), "Bulk shipment creation complete");
        Ok(BulkCreateResult { created, failed })
    }
}
