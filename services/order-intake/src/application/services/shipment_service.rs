//! ShipmentService — orchestrates order intake business logic.

use std::sync::Arc;
use chrono::Utc;
use logisticos_events::{Event, payloads::{AwbIssued, ShipmentCreated}, topics};
use logisticos_errors::{AppError, AppResult};
use logisticos_types::{
    awb::ServiceCode,
    ShipmentId, MerchantId, CustomerId, Money, Currency, ShipmentStatus, TenantId,
};
use crate::{
    application::commands::{
        CreateShipmentCommand, CancelShipmentCommand,
        BulkCreateShipmentCommand, BulkCreateResult, BulkRowError,
    },
    domain::{
        entities::{
            shipment::Shipment,
            piece::Piece,
        },
        value_objects::{
            ServiceType, ShipmentWeight, ShipmentDimensions,
            AwbGenerator, generate_child_awbs,
        },
    },
};

pub struct ShipmentListFilter {
    pub tenant_id:   uuid::Uuid,
    pub merchant_id: Option<uuid::Uuid>,
    pub status:      Option<String>,
    /// Inclusive lower bound on `updated_at` — used by billing queries to
    /// window shipments delivered within a billing period.
    pub updated_from: Option<chrono::DateTime<chrono::Utc>>,
    /// Exclusive upper bound on `updated_at`.
    pub updated_to:   Option<chrono::DateTime<chrono::Utc>>,
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

    fn save_pieces<'a>(
        &'a self,
        pieces: &'a [Piece],
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
    pub repo:          Arc<dyn ShipmentRepository>,
    pub publisher:     Arc<dyn EventPublisher>,
    pub normalizer:    Arc<dyn AddressNormalizer>,
    pub awb_generator: Arc<dyn AwbGenerator>,
}

impl ShipmentService {
    pub fn new(
        repo:          Arc<dyn ShipmentRepository>,
        publisher:     Arc<dyn EventPublisher>,
        normalizer:    Arc<dyn AddressNormalizer>,
        awb_generator: Arc<dyn AwbGenerator>,
    ) -> Self {
        Self { repo, publisher, normalizer, awb_generator }
    }

    pub async fn create(&self, cmd: CreateShipmentCommand) -> AppResult<Shipment> {
        tracing::info!(step = "enter", "ShipmentService::create");
        // ── Validate service type ────────────────────────────────────────────
        let service_type = match cmd.service_type.as_str() {
            "standard"      => ServiceType::Standard,
            "express"       => ServiceType::Express,
            "same_day"      => ServiceType::SameDay,
            "balikbayan"    => ServiceType::Balikbayan,
            "international" => ServiceType::International,
            other => return Err(AppError::Validation(format!("Unknown service type: {other}"))),
        };
        tracing::info!(step = "service_type_ok", ?service_type, "create");

        let service_code = match service_type {
            ServiceType::Standard      => ServiceCode::Standard,
            ServiceType::Express       => ServiceCode::Express,
            ServiceType::SameDay       => ServiceCode::SameDay,
            ServiceType::Balikbayan    => ServiceCode::Balikbayan,
            ServiceType::International => ServiceCode::International,
        };

        // ── Business rule: same-day cutoff at 14:00 ──────────────────────────
        if service_type == ServiceType::SameDay {
            let hour = Utc::now().format("%H").to_string().parse::<u32>().unwrap_or(0);
            if hour >= 14 {
                return Err(AppError::BusinessRule(
                    "Same-day orders must be placed before 14:00 local time".into(),
                ));
            }
        }

        // ── Validate weight ──────────────────────────────────────────────────
        let weight = ShipmentWeight::from_grams(cmd.weight_grams);
        weight.validate().map_err(|e| AppError::Validation(e.to_string()))?;

        // ── Business rule: COD must not exceed declared value ────────────────
        if let (Some(cod), Some(declared)) = (cmd.cod_amount_cents, cmd.declared_value_cents) {
            if cod > declared {
                return Err(AppError::BusinessRule(
                    "COD amount cannot exceed declared value".into(),
                ));
            }
        }

        // ── Piece count validation ───────────────────────────────────────────
        let piece_count = cmd.piece_count.unwrap_or(1).max(1).min(999);

        // ── Normalize and geocode addresses ──────────────────────────────────
        let origin      = self.normalizer.normalize(&cmd.origin).await.map_err(AppError::Internal)?;
        let destination = self.normalizer.normalize(&cmd.destination).await.map_err(AppError::Internal)?;
        tracing::info!(step = "normalized", "create");

        // ── Dimensions / billable weight ─────────────────────────────────────
        let dimensions = match (cmd.length_cm, cmd.width_cm, cmd.height_cm) {
            (Some(l), Some(w), Some(h)) => Some(ShipmentDimensions { length_cm: l, width_cm: w, height_cm: h }),
            _ => None,
        };
        let billable_grams = dimensions
            .map(|d| d.volumetric_weight_grams().max(weight.grams))
            .unwrap_or(weight.grams);

        // ── Generate master AWB ───────────────────────────────────────────────
        let tenant_code = logisticos_types::awb::TenantCode::new(&cmd.tenant_code)
            .map_err(|e| AppError::Validation(e.to_string()))?;
        tracing::info!(step = "tenant_code_ok", tenant_code = %tenant_code.as_str(), "create");
        let master_awb = self
            .awb_generator
            .next_awb(&tenant_code, service_code)
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!(e.to_string())))?;
        tracing::info!(step = "awb_generated", awb = %master_awb.as_str(), "create");

        // ── Generate child AWBs (one per piece) ───────────────────────────────
        let child_awbs = generate_child_awbs(&master_awb, piece_count)
            .map_err(|e| AppError::Internal(anyhow::anyhow!(e.to_string())))?;

        let now = Utc::now();

        // ── Build piece records ───────────────────────────────────────────────
        let piece_weight = ShipmentWeight::from_grams(
            // If individual piece weights not provided, distribute evenly
            weight.grams / piece_count as u32
        );
        let shipment_id = ShipmentId::new();

        let pieces: Vec<Piece> = child_awbs
            .iter()
            .map(|child| Piece {
                id: uuid::Uuid::new_v4(),
                shipment_id: shipment_id.clone(),
                piece_number: child.piece_number(),
                piece_awb: child.clone(),
                declared_weight: piece_weight,
                actual_weight: None,
                dimensions,
                description: cmd.description.clone(),
                status: logisticos_types::PieceStatus::Pending,
                last_hub_id: None,
                last_scanned_at: None,
                created_at: now,
                updated_at: now,
            })
            .collect();

        // ── Build shipment record ─────────────────────────────────────────────
        let shipment = Shipment {
            id: shipment_id.clone(),
            tenant_id: TenantId::from_uuid(cmd.tenant_id),
            merchant_id: MerchantId::from_uuid(cmd.merchant_id),
            customer_id: CustomerId::new(),
            customer_name: cmd.customer_name.clone(),
            customer_phone: cmd.customer_phone.clone(),
            customer_email: cmd.customer_email.clone(),
            booked_by_customer: cmd.booked_by_customer,
            awb: master_awb.clone(),
            piece_count,
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

        // ── Persist ───────────────────────────────────────────────────────────
        self.repo.save(&shipment).await.map_err(|e| {
            tracing::error!(error = ?e, "shipment_repo.save failed");
            AppError::Internal(e)
        })?;
        tracing::info!(step = "shipment_saved", "create");
        self.repo.save_pieces(&pieces).await.map_err(|e| {
            tracing::error!(error = ?e, "shipment_repo.save_pieces failed");
            AppError::Internal(e)
        })?;
        tracing::info!(step = "pieces_saved", "create");

        // ── Publish AwbIssued (fire-and-forget) ───────────────────────────────
        let awb_event = Event::new(
            "logisticos/order-intake",
            "awb.issued",
            cmd.tenant_id,
            AwbIssued {
                awb:          master_awb.as_str().to_string(),
                tenant_id:    cmd.tenant_id,
                shipment_id:  shipment.id.inner(),
                merchant_id:  cmd.merchant_id,
                service_code: service_code.as_str().to_string(),
                sequence:     master_awb.sequence(),
                piece_count,
                issued_at:    now.to_rfc3339(),
            },
        );
        if let Ok(payload) = serde_json::to_string(&awb_event) {
            let _ = self.publisher
                .publish(topics::AWB_ISSUED, master_awb.as_str(), &payload)
                .await;
        }

        // ── Publish ShipmentCreated (consumed by dispatch, engagement, analytics) ─
        let total_fee_cents = shipment.compute_base_fee().amount;
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
                customer_email:       cmd.customer_email.clone().unwrap_or_default(),
                origin_address:       format!("{}, {}", shipment.origin.city, shipment.origin.province),
                origin_city:          shipment.origin.city.clone(),
                origin_province:      shipment.origin.province.clone(),
                origin_postal_code:   shipment.origin.postal_code.clone(),
                origin_lat:           shipment.origin.coordinates.map(|c| c.lat),
                origin_lng:           shipment.origin.coordinates.map(|c| c.lng),
                destination_address:  format!("{}, {}", shipment.destination.city, shipment.destination.province),
                destination_city:     shipment.destination.city.clone(),
                destination_lat:      shipment.destination.coordinates.map(|c| c.lat),
                destination_lng:      shipment.destination.coordinates.map(|c| c.lng),
                service_type:         service_type.as_str().into(),
                cod_amount_cents:     shipment.cod_amount.map(|m| m.amount),
                tracking_number:      master_awb.as_str().to_string(),
                total_fee_cents,
                currency:             "PHP".into(),
                weight_grams:         billable_grams,
                estimated_delivery:   String::new(), // TODO: derive from service_type SLA
                booked_by_customer:   shipment.booked_by_customer,
            },
        );
        let payload = serde_json::to_string(&event).map_err(|e| AppError::Internal(e.into()))?;
        // Fire-and-forget — Kafka unavailability must not prevent shipment creation.
        if let Err(e) = self.publisher
            .publish(topics::SHIPMENT_CREATED, &shipment.id.to_string(), &payload)
            .await
        {
            tracing::warn!(error = %e, shipment_id = %shipment.id, "ShipmentCreated event publish failed (non-fatal)");
        }

        tracing::info!(
            shipment_id  = %shipment.id,
            awb          = %master_awb,
            piece_count,
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

        shipment.status     = ShipmentStatus::Cancelled;
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
