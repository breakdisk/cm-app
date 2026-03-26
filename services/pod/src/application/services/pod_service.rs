use std::sync::Arc;
use logisticos_errors::{AppError, AppResult};
use logisticos_types::{Coordinates, DriverId, TenantId};
use logisticos_events::{producer::KafkaProducer, topics, envelope::Event};
use uuid::Uuid;

use crate::{
    application::commands::{
        InitiatePodCommand, AttachSignatureCommand, AttachPhotoCommand,
        SubmitPodCommand, GenerateOtpCommand, VerifyOtpCommand, UploadUrlResponse,
    },
    domain::{
        entities::{ProofOfDelivery, PodPhoto, OtpCode},
        events::PodCaptured,
        repositories::{PodRepository, OtpRepository},
        value_objects::{
            POD_GEOFENCE_METERS, MAX_PHOTOS_PER_POD, MAX_PHOTO_SIZE_BYTES,
            is_allowed_content_type, generate_otp, hash_otp, verify_otp,
        },
    },
    infrastructure::external::storage::StorageAdapter,
    infrastructure::external::sms::SmsAdapter,
};

pub struct PodService {
    pod_repo: Arc<dyn PodRepository>,
    otp_repo: Arc<dyn OtpRepository>,
    storage: Arc<dyn StorageAdapter>,
    sms: Arc<dyn SmsAdapter>,
    kafka: Arc<KafkaProducer>,
}

impl PodService {
    pub fn new(
        pod_repo: Arc<dyn PodRepository>,
        otp_repo: Arc<dyn OtpRepository>,
        storage: Arc<dyn StorageAdapter>,
        sms: Arc<dyn SmsAdapter>,
        kafka: Arc<KafkaProducer>,
    ) -> Self {
        Self { pod_repo, otp_repo, storage, sms, kafka }
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
        // Idempotency: one POD per shipment
        if let Some(existing) = self.pod_repo.find_by_shipment(cmd.shipment_id).await.map_err(AppError::Internal)? {
            return Ok(existing);
        }

        // Geofence check — driver must be at the delivery address
        let driver_pos = Coordinates { lat: cmd.capture_lat, lng: cmd.capture_lng };
        let delivery_pos = Coordinates { lat: delivery_lat, lng: delivery_lng };
        let distance_m = driver_pos.distance_km(&delivery_pos) * 1000.0;
        let geofence_verified = distance_m <= POD_GEOFENCE_METERS;

        tracing::info!(
            driver_id = %driver_id,
            distance_m = %distance_m,
            geofence_verified = %geofence_verified,
            "POD geofence check"
        );

        let pod = ProofOfDelivery::new(
            tenant_id.inner(),
            cmd.shipment_id,
            cmd.task_id,
            driver_id.inner(),
            cmd.recipient_name,
            cmd.capture_lat,
            cmd.capture_lng,
            geofence_verified,
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

        let upload_url = self.storage
            .presign_upload(&s3_key, content_type, 30)
            .await
            .map_err(AppError::Internal)?;

        Ok(UploadUrlResponse { upload_url, s3_key })
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
                .find_active_by_shipment(pod.shipment_id).await
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

        // Publish POD captured event — payments service reconciles COD,
        // engagement sends delivery confirmation to customer
        let event = Event::new("pod", "pod.captured", tenant_id.inner(), PodCaptured {
            pod_id: pod.id,
            shipment_id: pod.shipment_id,
            task_id: pod.task_id,
            tenant_id: tenant_id.inner(),
            driver_id: driver_id.inner(),
            recipient_name: pod.recipient_name.clone(),
            has_signature: pod.signature_data.is_some(),
            photo_count: pod.photos.len(),
            otp_verified: pod.otp_verified,
            cod_collected_cents: pod.cod_collected_cents,
            captured_at: pod.captured_at,
        });
        self.kafka.publish_event(topics::POD_CAPTURED, &event).await
            .map_err(AppError::Internal)?;

        tracing::info!(
            pod_id = %pod_id,
            shipment_id = %pod.shipment_id,
            driver_id = %driver_id,
            "POD submitted"
        );
        Ok(pod_id)
    }

    /// Generate and send OTP to recipient's phone for high-value deliveries.
    /// Should be called by driver before arriving at address.
    pub async fn generate_and_send_otp(
        &self,
        tenant_id: &TenantId,
        cmd: GenerateOtpCommand,
    ) -> AppResult<Uuid> {
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

        // Send via SMS — engagement service owns the template, we just send the raw code here
        let message = format!("Your LogisticOS delivery code is: {code}. Valid for 15 minutes. Do not share.");
        self.sms.send(&cmd.recipient_phone, &message).await
            .map_err(AppError::Internal)?;

        tracing::info!(
            shipment_id = %cmd.shipment_id,
            phone = %cmd.recipient_phone,
            "OTP sent"
        );
        Ok(otp_id)
    }

    /// Retrieve a POD record by ID (for admin/ops views).
    pub async fn get_by_id(&self, pod_id: Uuid) -> AppResult<ProofOfDelivery> {
        self.load_pod(pod_id).await
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
