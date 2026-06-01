use std::sync::Arc;
use logisticos_errors::{AppError, AppResult};
use logisticos_types::{Coordinates, DriverId, TenantId};
use logisticos_events::{producer::KafkaProducer, topics, envelope::Event};
use uuid::Uuid;

use crate::{
    application::commands::{
        InitiatePodCommand, AttachSignatureCommand, AttachPhotoCommand,
        SubmitPodCommand, GenerateOtpCommand, VerifyOtpCommand, UploadUrlResponse,
        InitiatePickupCommand, SubmitPickupCommand,
    },
    domain::{
        entities::{ProofOfDelivery, PodPhoto, OtpCode, ProofOfPickup},
        events::{PodCaptured, PickupCaptured},
        repositories::{PodRepository, OtpRepository, PickupRepository, TelemetryRepository, TelemetryEntry},
        value_objects::{
            POD_GEOFENCE_METERS, OUT_OF_BOUNDS_HANDOVER_METERS,
            MAX_PHOTOS_PER_POD, MAX_PHOTO_SIZE_BYTES,
            is_allowed_content_type, generate_otp, hash_otp, verify_otp,
        },
    },
    infrastructure::external::storage::StorageAdapter,
    infrastructure::external::sms::SmsAdapter,
};

pub struct PodService {
    pod_repo:     Arc<dyn PodRepository>,
    otp_repo:     Arc<dyn OtpRepository>,
    pickup_repo:  Arc<dyn PickupRepository>,
    telemetry:    Arc<dyn TelemetryRepository>,
    /// Bucket for Proof-of-Delivery photos (`logisticos-pod-photos`).
    pod_storage:  Arc<dyn StorageAdapter>,
    /// Separate bucket for Proof-of-Pickup photos (`logisticos-pop-photos`).
    /// Keeps pickup evidence partitioned from delivery evidence in R2.
    pop_storage:  Arc<dyn StorageAdapter>,
    sms:          Arc<dyn SmsAdapter>,
    kafka:        Arc<KafkaProducer>,
}

impl PodService {
    pub fn new(
        pod_repo:    Arc<dyn PodRepository>,
        otp_repo:    Arc<dyn OtpRepository>,
        pickup_repo: Arc<dyn PickupRepository>,
        telemetry:   Arc<dyn TelemetryRepository>,
        pod_storage: Arc<dyn StorageAdapter>,
        pop_storage: Arc<dyn StorageAdapter>,
        sms:         Arc<dyn SmsAdapter>,
        kafka:       Arc<KafkaProducer>,
    ) -> Self {
        Self { pod_repo, otp_repo, pickup_repo, telemetry, pod_storage, pop_storage, sms, kafka }
    }

    /// Step 1: Driver initiates POD capture at delivery location.
    /// GPS coordinates are validated against the delivery address geofence.
    pub async fn initiate(
        &self,
        driver_id: &DriverId,
        tenant_id: &TenantId,
        cmd: InitiatePodCommand,
        delivery_lat: f64,
        delivery_lng: f64,
    ) -> AppResult<ProofOfDelivery> {
        // Idempotency: one *active* POD per shipment per driver. If a POD
        // already exists for this shipment, return it only when the calling
        // driver owns it — same driver hitting Submit twice or replaying
        // initiate after a network blip should not create duplicates.
        //
        // If the existing POD belongs to a *different* driver (e.g. the
        // shipment was reassigned after a stale orphan-pending assignment was
        // cancelled and re-dispatched), the prior draft is now stranded.
        // Returning it would route the new driver's submit to a row owned
        // by the old driver and the ownership check on submit would 403.
        // Instead we error out and let an operator delete the stale draft —
        // surfacing the inconsistency rather than silently overwriting it.
        if let Some(existing) = self.pod_repo.find_by_shipment(cmd.shipment_id).await.map_err(AppError::Internal)? {
            if existing.driver_id == driver_id.inner() {
                return Ok(existing);
            }
            return Err(AppError::BusinessRule(format!(
                "POD for shipment {} already initiated by another driver ({}). \
                 Ask ops to clear the stale draft before retrying.",
                cmd.shipment_id, existing.driver_id
            )));
        }

        // Geofence check — driver must be at the delivery address
        let driver_pos = Coordinates { lat: cmd.capture_lat, lng: cmd.capture_lng };
        let delivery_pos = Coordinates { lat: delivery_lat, lng: delivery_lng };
        let distance_m = driver_pos.distance_km(&delivery_pos) * 1000.0;
        let geofence_verified = distance_m <= POD_GEOFENCE_METERS;

        // OUT_OF_BOUNDS_HANDOVER — soft audit flag for distances > 50m.
        // Non-blocking: driver can still submit. Recorded on the entity and forwarded
        // in PodCaptured so payments billing can write it to workflow_metadata.
        let out_of_bounds_handover = distance_m > OUT_OF_BOUNDS_HANDOVER_METERS;

        tracing::info!(
            driver_id              = %driver_id,
            distance_m             = %distance_m,
            geofence_verified      = %geofence_verified,
            out_of_bounds_handover = %out_of_bounds_handover,
            "POD geofence check"
        );

        if out_of_bounds_handover {
            tracing::warn!(
                driver_id   = %driver_id,
                distance_m  = %distance_m,
                shipment_id = %cmd.shipment_id,
                "OUT_OF_BOUNDS_HANDOVER: driver is >{}m from delivery address; \
                 audit flag set on POD (non-blocking)",
                OUT_OF_BOUNDS_HANDOVER_METERS as u32
            );
        }

        let pod = ProofOfDelivery::new(
            tenant_id.inner(),
            cmd.shipment_id,
            cmd.task_id,
            driver_id.inner(),
            cmd.recipient_name,
            cmd.capture_lat,
            cmd.capture_lng,
            geofence_verified,
            out_of_bounds_handover,
            cmd.device_timestamp,
            cmd.requires_photo,
            cmd.requires_signature,
        );

        self.pod_repo.save(&pod).await.map_err(AppError::Internal)?;
        Ok(pod)
    }

    /// Step 2a: Attach recipient signature to the POD.
    pub async fn attach_signature(&self, cmd: AttachSignatureCommand) -> AppResult<()> {
        let mut pod = self.load_pod(cmd.pod_id).await?;
        self.assert_draft(&pod)?;

        // Basic size check — signature data shouldn't exceed 500KB (compressed SVG/PNG)
        if cmd.signature_data.len() > 500 * 1024 {
            return Err(AppError::Validation("Signature data too large (max 500KB)".into()));
        }

        pod.attach_signature(cmd.signature_data);
        self.pod_repo.save(&pod).await.map_err(AppError::Internal)
    }

    /// Step 2b: Generate a pre-signed S3 upload URL for a delivery photo.
    /// Driver app uploads directly to S3; after upload, calls attach_photo.
    pub async fn get_upload_url(
        &self,
        pod_id: Uuid,
        tenant_id: &TenantId,
        content_type: &str,
    ) -> AppResult<UploadUrlResponse> {
        if !is_allowed_content_type(content_type) {
            return Err(AppError::Validation(format!(
                "Content type '{content_type}' not allowed. Use image/jpeg, image/png, or image/webp"
            )));
        }

        let pod = self.load_pod(pod_id).await?;
        self.assert_draft(&pod)?;

        if pod.photos.len() >= MAX_PHOTOS_PER_POD {
            return Err(AppError::BusinessRule(format!(
                "Maximum of {MAX_PHOTOS_PER_POD} photos per POD"
            )));
        }

        let s3_key = format!(
            "pod/{}/{}/{}/{}.{}",
            tenant_id.inner(),
            pod.shipment_id,
            pod_id,
            Uuid::new_v4(),
            if content_type.contains("png") { "png" } else if content_type.contains("webp") { "webp" } else { "jpg" }
        );

        let presigned = self.pod_storage
            .presign_upload(&s3_key, content_type, 900)
            .await
            .map_err(AppError::Internal)?;

        Ok(UploadUrlResponse {
            upload_url: presigned.url,
            s3_key,
            upload_headers: presigned.headers,
        })
    }

    /// Step 2c: Register a completed photo upload (called after driver finishes S3 PUT).
    pub async fn attach_photo(&self, cmd: AttachPhotoCommand) -> AppResult<()> {
        let mut pod = self.load_pod(cmd.pod_id).await?;
        self.assert_draft(&pod)?;

        if cmd.size_bytes > MAX_PHOTO_SIZE_BYTES {
            return Err(AppError::Validation(format!(
                "Photo too large: {}MB max", MAX_PHOTO_SIZE_BYTES / 1_048_576
            )));
        }

        if !is_allowed_content_type(&cmd.content_type) {
            return Err(AppError::Validation("Invalid photo content type".into()));
        }

        pod.attach_photo(PodPhoto {
            id: Uuid::new_v4(),
            s3_key: cmd.s3_key,
            content_type: cmd.content_type,
            size_bytes: cmd.size_bytes,
            uploaded_at: chrono::Utc::now(),
        });

        self.pod_repo.save(&pod).await.map_err(AppError::Internal)
    }

    /// Step 3: Submit the completed POD. Validates all required evidence is present,
    /// verifies OTP if provided, records COD collection, publishes event.
    pub async fn submit(
        &self,
        driver_id: &DriverId,
        tenant_id: &TenantId,
        cmd: SubmitPodCommand,
    ) -> AppResult<Uuid> {
        let mut pod = self.load_pod(cmd.pod_id).await?;
        self.assert_draft(&pod)?;

        // Validate driver owns this POD
        if pod.driver_id != driver_id.inner() {
            return Err(AppError::Forbidden { resource: "POD".into() });
        }

        // OTP verification — validate if code provided
        if let Some(otp_code) = cmd.otp_code {
            let otp = self.otp_repo
                .find_active_by_shipment(pod.shipment_id, tenant_id.inner()).await
                .map_err(AppError::Internal)?;

            match otp {
                None => return Err(AppError::BusinessRule("No active OTP found for this shipment".into())),
                Some(mut otp_record) => {
                    if !otp_record.is_valid() {
                        return Err(AppError::BusinessRule("OTP has expired. Request a new one".into()));
                    }
                    if !verify_otp(&otp_code, &otp_record.code_hash) {
                        return Err(AppError::BusinessRule("Invalid OTP code".into()));
                    }
                    pod.mark_otp_verified(otp_record.id);
                    otp_record.mark_used();
                    self.otp_repo.save(&otp_record).await.map_err(AppError::Internal)?;
                }
            }
        }

        // Record COD collection
        if let Some(amount) = cmd.cod_collected_cents {
            pod.record_cod(amount);
        }

        // Finalize — validates evidence completeness
        pod.submit().map_err(|e| AppError::BusinessRule(e.to_string()))?;
        let pod_id = pod.id;
        self.pod_repo.save(&pod).await.map_err(AppError::Internal)?;

        // Publish POD captured event — payments service reconciles COD and
        // issues a payment receipt for customer-booked shipments.
        // Fire-and-forget: POD is already persisted; a Kafka outage must not
        // fail the driver's submit. Missed events are recovered by reconciliation.
        let event = Event::new("pod", "pod.captured", tenant_id.inner(), PodCaptured {
            pod_id:                 pod.id,
            shipment_id:            pod.shipment_id,
            task_id:                pod.task_id,
            tenant_id:              tenant_id.inner(),
            driver_id:              driver_id.inner(),
            recipient_name:         pod.recipient_name.clone(),
            has_signature:          pod.signature_data.is_some(),
            photo_count:            pod.photos.len(),
            otp_verified:           pod.otp_verified,
            cod_amount_cents:       pod.cod_collected_cents.unwrap_or(0),
            captured_at:            pod.captured_at.to_rfc3339(),
            // device_timestamp forwarded for chain-of-custody audit; absent on older clients.
            device_timestamp:       pod.device_timestamp.map(|t| t.to_rfc3339()),
            // Soft audit flag written to billing workflow_metadata by payments consumer.
            out_of_bounds_handover: pod.out_of_bounds_handover,
            tenant_code:            cmd.tenant_code.clone(),
            booked_by_customer:     cmd.booked_by_customer,
            customer_id:            cmd.customer_id,
            customer_email:         cmd.customer_email.clone(),
            // customer_phone from driver app task screen — enables payments/engagement
            // to send WhatsApp receipts without a cross-service customer lookup.
            customer_phone:         cmd.customer_phone.clone(),
        });
        if let Err(e) = self.kafka.publish_event(topics::POD_CAPTURED, &event).await {
            tracing::error!(
                error = %e,
                pod_id = %pod_id,
                shipment_id = %pod.shipment_id,
                "POD_CAPTURED event publish failed — POD saved, event will be reconciled"
            );
        }

        // Append telemetry log entry (fire-and-forget — never block the POD submit).
        let device_ts = cmd.device_timestamp.unwrap_or(pod.captured_at);
        let telemetry_meta = if pod.out_of_bounds_handover {
            serde_json::json!({
                "telemetry_exception": "OUT_OF_BOUNDS_HANDOVER",
                "geofence_verified": pod.geofence_verified,
            })
        } else {
            serde_json::json!({
                "geofence_verified": pod.geofence_verified,
            })
        };
        if let Err(e) = self.telemetry.append(TelemetryEntry {
            tenant_id:        tenant_id.inner(),
            shipment_id:      pod.shipment_id,
            task_id:          Some(pod.task_id),
            driver_id:        Some(driver_id.inner()),
            event_type:       "pod_submitted".into(),
            device_timestamp: device_ts,
            server_timestamp: chrono::Utc::now(),
            lat:              Some(pod.capture_lat),
            lng:              Some(pod.capture_lng),
            metadata:         telemetry_meta,
        }).await {
            tracing::warn!(error = %e, pod_id = %pod_id, "Failed to append pod_submitted telemetry — non-fatal");
        }

        tracing::info!(
            pod_id = %pod_id,
            shipment_id = %pod.shipment_id,
            driver_id = %driver_id,
            out_of_bounds_handover = %pod.out_of_bounds_handover,
            "POD submitted"
        );
        Ok(pod_id)
    }

    // ── Proof of Pickup ────────────────────────────────────────────────────────

    /// Step P1: Driver initiates a Proof of Pickup at the merchant/hub.
    /// Performs geofence + OUT_OF_BOUNDS_HANDOVER checks against the pickup address.
    pub async fn initiate_pickup(
        &self,
        driver_id: &DriverId,
        tenant_id: &TenantId,
        cmd: InitiatePickupCommand,
    ) -> AppResult<ProofOfPickup> {
        // Idempotency: return existing draft if this driver already started one.
        if let Some(existing) = self.pickup_repo.find_by_shipment(cmd.shipment_id).await.map_err(AppError::Internal)? {
            if existing.driver_id == driver_id.inner() {
                return Ok(existing);
            }
            return Err(AppError::BusinessRule(format!(
                "POP for shipment {} already initiated by another driver. Ask ops to clear the draft.",
                cmd.shipment_id
            )));
        }

        let driver_pos  = Coordinates { lat: cmd.capture_lat, lng: cmd.capture_lng };
        let pickup_pos  = Coordinates { lat: cmd.pickup_lat,  lng: cmd.pickup_lng  };
        let distance_m  = driver_pos.distance_km(&pickup_pos) * 1000.0;
        let geofence_verified       = distance_m <= POD_GEOFENCE_METERS;
        let out_of_bounds_handover  = distance_m > OUT_OF_BOUNDS_HANDOVER_METERS;

        tracing::info!(
            driver_id              = %driver_id,
            distance_m             = %distance_m,
            geofence_verified      = %geofence_verified,
            out_of_bounds_handover = %out_of_bounds_handover,
            "POP geofence check"
        );

        if out_of_bounds_handover {
            tracing::warn!(
                driver_id   = %driver_id,
                distance_m  = %distance_m,
                shipment_id = %cmd.shipment_id,
                "OUT_OF_BOUNDS_HANDOVER: driver is >{}m from pickup address; \
                 audit flag set on POP (non-blocking)",
                OUT_OF_BOUNDS_HANDOVER_METERS as u32
            );
        }

        let pop = ProofOfPickup::new(
            tenant_id.inner(),
            cmd.shipment_id,
            cmd.task_id,
            driver_id.inner(),
            cmd.capture_lat,
            cmd.capture_lng,
            geofence_verified,
            out_of_bounds_handover,
            cmd.declared_weight_g,
            cmd.service_code,
            cmd.declared_value_cents,
            cmd.device_timestamp,
        );

        self.pickup_repo.save(&pop).await.map_err(AppError::Internal)?;
        Ok(pop)
    }

    /// Step P2: Driver submits a completed Proof of Pickup.
    /// Validates barcode, records weight, publishes PickupCaptured event.
    pub async fn submit_pickup(
        &self,
        driver_id: &DriverId,
        tenant_id: &TenantId,
        cmd:       SubmitPickupCommand,
        tenant_code: String,
    ) -> AppResult<Uuid> {
        let mut pop = self.pickup_repo.find_by_id(cmd.pop_id).await.map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound { resource: "POP", id: cmd.pop_id.to_string() })?;

        if pop.driver_id != driver_id.inner() {
            return Err(AppError::Forbidden { resource: "POP".into() });
        }
        if pop.status == crate::domain::entities::PopStatus::Submitted {
            return Err(AppError::BusinessRule("POP has already been submitted".into()));
        }

        pop.record_scan(cmd.scanned_barcode.clone());

        if let Some(actual_g) = cmd.actual_weight_g {
            let declared_g = pop.declared_weight_g.unwrap_or(actual_g);
            pop.record_weight(actual_g, declared_g);
        }

        if let Some(ref key) = cmd.photo_s3_key {
            pop.attach_photo(key.clone(), cmd.photo_size_bytes.unwrap_or(0));
        }

        // AR dimensioning (driver app VERIFY mode) — persisted for the POP
        // size/quantity/anti-fraud audit. All fields optional.
        if cmd.verified_length_cm.is_some() || cmd.box_quantity.is_some() {
            pop.record_dimensions(
                cmd.verified_length_cm,
                cmd.verified_width_cm,
                cmd.verified_height_cm,
                cmd.verified_cbm,
                cmd.volumetric_weight_kg,
                cmd.box_quantity,
                cmd.dimension_integrity.clone(),
            );
        }

        // Override device_timestamp if provided at submit time (more accurate).
        if cmd.device_timestamp.is_some() {
            pop.device_timestamp = cmd.device_timestamp;
        }

        pop.submit();
        let pop_id = pop.id;
        self.pickup_repo.save(&pop).await.map_err(AppError::Internal)?;

        // Append telemetry log (fire-and-forget).
        let device_ts = pop.device_timestamp.unwrap_or(pop.captured_at);
        let mut tele_meta = serde_json::json!({
            "scanned_barcode": pop.scanned_barcode,
            "geofence_verified": pop.geofence_verified,
        });
        if pop.out_of_bounds_handover {
            tele_meta["telemetry_exception"] = serde_json::json!("OUT_OF_BOUNDS_HANDOVER");
        }
        if let Some(ratio) = pop.weight_overage_ratio() {
            tele_meta["weight_overage_ratio"] = serde_json::json!(ratio);
        }
        // AR dimensioning audit trail (anti-fraud / size / quantity).
        if pop.verified_length_cm.is_some() || pop.box_quantity.is_some() {
            tele_meta["ar_dimensions"] = serde_json::json!({
                "length_cm":            pop.verified_length_cm,
                "width_cm":             pop.verified_width_cm,
                "height_cm":            pop.verified_height_cm,
                "cbm":                  pop.verified_cbm,
                "volumetric_weight_kg": pop.volumetric_weight_kg,
                "box_quantity":         pop.box_quantity,
                "integrity":            pop.dimension_integrity,
            });
        }
        if let Err(e) = self.telemetry.append(TelemetryEntry {
            tenant_id:        tenant_id.inner(),
            shipment_id:      pop.shipment_id,
            task_id:          Some(pop.task_id),
            driver_id:        Some(driver_id.inner()),
            event_type:       "pop_submitted".into(),
            device_timestamp: device_ts,
            server_timestamp: chrono::Utc::now(),
            lat:              Some(pop.capture_lat),
            lng:              Some(pop.capture_lng),
            metadata:         tele_meta,
        }).await {
            tracing::warn!(error = %e, pop_id = %pop_id, "Failed to append pop_submitted telemetry — non-fatal");
        }

        // Publish PickupCaptured — driver-ops marks task IN_PROGRESS; payments opens
        // the billing chain of custody for Track A/B invoicing.
        let event = Event::new("pod", "pickup.captured", tenant_id.inner(), PickupCaptured {
            pop_id:               pop.id,
            shipment_id:          pop.shipment_id,
            task_id:              pop.task_id,
            tenant_id:            tenant_id.inner(),
            driver_id:            driver_id.inner(),
            geofence_verified:    pop.geofence_verified,
            out_of_bounds_handover: pop.out_of_bounds_handover,
            barcode_scanned:      pop.barcode_scanned,
            // Billing routing — "balikbayan" triggers Track A driver ledger debit.
            service_code:         pop.service_code.clone(),
            declared_value_cents: pop.declared_value_cents,
            actual_weight_g:      pop.actual_weight_g,
            declared_weight_g:    pop.declared_weight_g,
            weight_overage_ratio: pop.weight_overage_ratio(),
            photo_s3_key:         pop.photo_s3_key.clone(),
            captured_at:          pop.captured_at.to_rfc3339(),
            device_timestamp:     pop.device_timestamp.map(|t| t.to_rfc3339()),
            tenant_code,
        });
        if let Err(e) = self.kafka.publish_event(topics::PICKUP_CAPTURED, &event).await {
            tracing::error!(
                error = %e,
                pop_id = %pop_id,
                "PICKUP_CAPTURED event publish failed — POP saved, event will be reconciled"
            );
        }

        tracing::info!(pop_id = %pop_id, shipment_id = %pop.shipment_id, driver_id = %driver_id, "POP submitted");
        Ok(pop_id)
    }

    /// Step P1b: Generate a pre-signed S3 upload URL for a pickup photo.
    ///
    /// Mirrors [`get_upload_url`] for POD photos, but scoped to a POP record and
    /// using a `pop/` key prefix so photos are partitioned from delivery evidence.
    /// The returned `s3_key` is passed in [`SubmitPickupCommand::photo_s3_key`] —
    /// there is no separate "attach photo" step for POP (single-photo, no limit check).
    pub async fn get_pop_upload_url(
        &self,
        pop_id: Uuid,
        tenant_id: &TenantId,
        content_type: &str,
    ) -> AppResult<UploadUrlResponse> {
        if !is_allowed_content_type(content_type) {
            return Err(AppError::Validation(format!(
                "Content type '{content_type}' not allowed. Use image/jpeg, image/png, or image/webp"
            )));
        }

        let pop = self.pickup_repo.find_by_id(pop_id).await.map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound { resource: "POP", id: pop_id.to_string() })?;

        if pop.status == crate::domain::entities::PopStatus::Submitted {
            return Err(AppError::BusinessRule("POP has already been submitted — cannot attach photo".into()));
        }

        let ext = if content_type.contains("png") { "png" }
            else if content_type.contains("webp") { "webp" }
            else { "jpg" };

        let s3_key = format!(
            "pop/{}/{}/{}/{}.{}",
            tenant_id.inner(),
            pop.shipment_id,
            pop_id,
            Uuid::new_v4(),
            ext,
        );

        let presigned = self.pop_storage
            .presign_upload(&s3_key, content_type, 900)
            .await
            .map_err(AppError::Internal)?;

        Ok(UploadUrlResponse {
            upload_url: presigned.url,
            s3_key,
            upload_headers: presigned.headers,
        })
    }

    /// Generate and send OTP to recipient's phone for high-value deliveries.
    /// Should be called by driver before arriving at address.
    ///
    /// Returns `(otp_id, code)`. The plaintext code is always included so the
    /// driver app can display it as a fallback when no SMS arrives (e.g. staging
    /// with no Twilio configured, or the recipient has no mobile signal).
    /// The endpoint is already auth-gated to drivers so surfacing it here is safe.
    pub async fn generate_and_send_otp(
        &self,
        tenant_id: &TenantId,
        cmd: GenerateOtpCommand,
    ) -> AppResult<(Uuid, String)> {
        // Invalidate any previous OTP for this shipment by letting it expire (no delete needed —
        // find_active_by_shipment filters by is_used=false AND expires_at > NOW())
        let code = generate_otp();
        let code_hash = hash_otp(&code);

        let otp = OtpCode::new(
            tenant_id.inner(),
            cmd.shipment_id,
            cmd.recipient_phone.clone(),
            code_hash,
        );
        let otp_id = otp.id;

        self.otp_repo.save(&otp).await.map_err(AppError::Internal)?;

        // SMS is best-effort. A Twilio failure (wrong creds, network, rate-limit)
        // must NOT prevent the OTP from being issued — the driver app displays
        // data.code as a fallback so the delivery can still be completed.
        let message = format!("Your LogisticOS delivery code is: {code}. Valid for 15 minutes. Do not share.");
        match self.sms.send(&cmd.recipient_phone, &message).await {
            Ok(()) => {
                tracing::info!(
                    shipment_id = %cmd.shipment_id,
                    phone       = %cmd.recipient_phone,
                    "OTP dispatched via SMS"
                );
            }
            Err(e) => {
                // Log at ERROR so it shows up in monitoring, but return 200 so
                // the driver app can proceed using the code from the response.
                tracing::error!(
                    error       = %e,
                    shipment_id = %cmd.shipment_id,
                    phone       = %cmd.recipient_phone,
                    "SMS delivery failed — OTP still valid; code returned in API response"
                );
            }
        }

        Ok((otp_id, code))
    }

    /// Retrieve a POD record by ID (for admin/ops views).
    pub async fn get_by_id(&self, pod_id: Uuid) -> AppResult<ProofOfDelivery> {
        self.load_pod(pod_id).await
    }

    /// Retrieve a POP record by ID (for admin/ops views).
    pub async fn get_pop_by_id(&self, pop_id: Uuid) -> AppResult<ProofOfPickup> {
        self.pickup_repo.find_by_id(pop_id).await.map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound { resource: "POP", id: pop_id.to_string() })
    }

    /// Retrieve the most recent POP for a shipment (for admin portal panel).
    pub async fn get_pop_by_shipment(&self, shipment_id: Uuid) -> AppResult<Option<ProofOfPickup>> {
        self.pickup_repo.find_by_shipment(shipment_id).await.map_err(AppError::Internal)
    }

    /// Retrieve the *completed* (submitted) POP for a shipment.
    /// Returns `None` when pickup has not yet been captured or is still in draft.
    /// Used by `pop_status_internal` and the carrier container-load guard.
    pub async fn get_completed_pop_by_shipment(&self, shipment_id: Uuid) -> AppResult<Option<ProofOfPickup>> {
        self.pickup_repo.find_completed_by_shipment(shipment_id).await.map_err(AppError::Internal)
    }

    /// Retrieve the most recent POD for a shipment (for admin portal panel).
    pub async fn get_by_shipment(&self, shipment_id: Uuid) -> AppResult<Option<ProofOfDelivery>> {
        self.pod_repo.find_by_shipment(shipment_id).await.map_err(AppError::Internal)
    }

    /// Map a POP entity to the JSON shape expected by the admin portal.
    /// Generates a presigned download URL for the pickup photo (1-hour TTL).
    pub async fn pop_to_view(&self, pop: &ProofOfPickup) -> serde_json::Value {
        let photo_url: Option<String> = if let Some(ref key) = pop.photo_s3_key {
            self.pop_storage.presign_download(key, 3600).await
                .map_err(|e| tracing::warn!(error = %e, s3_key = %key, "Failed to presign POP photo"))
                .ok()
        } else {
            None
        };

        let status_str = if pop.status == crate::domain::entities::PopStatus::Submitted {
            "submitted"
        } else {
            "draft"
        };

        serde_json::json!({
            "pop_id":                 pop.id,
            "shipment_id":            pop.shipment_id,
            "task_id":                pop.task_id,
            "driver_id":              pop.driver_id,
            "status":                 status_str,
            "capture_lat":            pop.capture_lat,
            "capture_lng":            pop.capture_lng,
            "geofence_verified":      pop.geofence_verified,
            "out_of_bounds_handover": pop.out_of_bounds_handover,
            "barcode_scanned":        pop.barcode_scanned,
            "scanned_barcode":        pop.scanned_barcode,
            "actual_weight_g":        pop.actual_weight_g,
            "declared_weight_g":      pop.declared_weight_g,
            "weight_overage_ratio":   pop.weight_overage_ratio(),
            "verified_length_cm":     pop.verified_length_cm,
            "verified_width_cm":      pop.verified_width_cm,
            "verified_height_cm":     pop.verified_height_cm,
            "verified_cbm":           pop.verified_cbm,
            "volumetric_weight_kg":   pop.volumetric_weight_kg,
            "box_quantity":           pop.box_quantity,
            "dimension_integrity":    pop.dimension_integrity,
            "photo_url":              photo_url,
            "device_timestamp":       pop.device_timestamp,
            "captured_at":            pop.captured_at,
        })
    }

    /// Map a POD entity to the merchant/customer evidence shape.
    /// Presigns ALL photos into `photo_urls: Vec<String>` (not just the first).
    /// Used by the internal `/v1/internal/pop-evidence/:id` endpoint so the
    /// delivery-experience service can embed full photo evidence in tracking responses.
    pub async fn pod_evidence_to_view(&self, pod: &ProofOfDelivery) -> serde_json::Value {
        let mut photo_urls: Vec<String> = Vec::new();
        for photo in &pod.photos {
            match self.pod_storage.presign_download(&photo.s3_key, 3600).await {
                Ok(url) => photo_urls.push(url),
                Err(e)  => tracing::warn!(
                    error = %e,
                    s3_key = %photo.s3_key,
                    "Failed to presign POD photo for evidence view — skipping"
                ),
            }
        }

        let signature_url = pod.signature_data.as_ref().map(|b64| {
            format!("data:image/png;base64,{b64}")
        });

        serde_json::json!({
            "photo_urls":        photo_urls,
            "signature_url":     signature_url,
            "delivered_at":      pod.captured_at,
            "recipient_name":    pod.recipient_name,
            "geofence_verified": pod.geofence_verified,
        })
    }

    /// Map a POD entity to the JSON shape expected by the admin portal.
    /// Generates a presigned download URL for the first photo (1-hour TTL)
    /// and converts signature base64 to a data URI the browser can render.
    pub async fn pod_to_view(&self, pod: &ProofOfDelivery) -> serde_json::Value {
        let photo_url: Option<String> = if let Some(photo) = pod.photos.first() {
            self.pod_storage.presign_download(&photo.s3_key, 3600).await
                .map_err(|e| tracing::warn!(error = %e, s3_key = %photo.s3_key, "Failed to presign POD photo"))
                .ok()
        } else {
            None
        };

        let signature_url = pod.signature_data.as_ref().map(|b64| {
            format!("data:image/png;base64,{b64}")
        });

        let status_str = match pod.status {
            crate::domain::entities::PodStatus::Draft     => "draft",
            crate::domain::entities::PodStatus::Submitted => "submitted",
            crate::domain::entities::PodStatus::Verified  => "verified",
            crate::domain::entities::PodStatus::Disputed  => "disputed",
        };

        serde_json::json!({
            "pod_id":              pod.id,
            "shipment_id":         pod.shipment_id,
            "task_id":             pod.task_id,
            "status":              status_str,
            "recipient_name":      pod.recipient_name,
            "geofence_verified":   pod.geofence_verified,
            "capture_lat":         pod.capture_lat,
            "capture_lng":         pod.capture_lng,
            "photo_url":           photo_url,
            "signature_url":       signature_url,
            "otp_verified":        pod.otp_verified,
            "cod_collected_cents": pod.cod_collected_cents,
            "captured_at":         pod.captured_at,
        })
    }

    /// Standalone OTP verification — driver can pre-verify before submitting POD.
    /// Returns otp_id on success.
    pub async fn verify_otp_standalone(
        &self,
        tenant_id: Uuid,
        cmd: VerifyOtpCommand,
    ) -> AppResult<Uuid> {
        let otp = self.otp_repo
            .find_active_by_shipment(cmd.shipment_id, tenant_id).await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::BusinessRule("No active OTP found for this shipment".into()))?;

        if !otp.is_valid() {
            return Err(AppError::BusinessRule("OTP has expired. Request a new one.".into()));
        }

        if !verify_otp(&cmd.code, &otp.code_hash) {
            return Err(AppError::BusinessRule("Invalid OTP code".into()));
        }

        tracing::info!(shipment_id = %cmd.shipment_id, "OTP pre-verified");
        Ok(otp.id)
    }

    async fn load_pod(&self, pod_id: Uuid) -> AppResult<ProofOfDelivery> {
        self.pod_repo.find_by_id(pod_id).await.map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound { resource: "POD", id: pod_id.to_string() })
    }

    fn assert_draft(&self, pod: &ProofOfDelivery) -> AppResult<()> {
        use crate::domain::entities::PodStatus;
        if pod.status != PodStatus::Draft {
            return Err(AppError::BusinessRule("POD has already been submitted".into()));
        }
        Ok(())
    }
}
