use std::sync::Arc;
use uuid::Uuid;

use logisticos_errors::{AppError, AppResult};
use logisticos_types::TenantId;

const FEEDBACK_MAX_TAGS: usize = 10;
const FEEDBACK_MAX_TAG_CHARS: usize = 50;
const FEEDBACK_MAX_COMMENT_CHARS: usize = 1000;

use crate::domain::{
    entities::{TrackingRecord, TrackingStatus},
    repositories::{NewFeedback, TrackingRepository},
    value_objects::{may_read, Reader},
};

/// Abstraction over the Kafka producer so the service layer can emit events
/// without depending on rdkafka directly. Production binding lives in
/// [`crate::infrastructure::messaging::KafkaEventPublisher`].
pub trait EventPublisher: Send + Sync {
    fn publish<'a>(
        &'a self,
        topic: &'a str,
        key: &'a str,
        payload: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + 'a>>;
}

pub struct TrackingService {
    repo: Arc<dyn TrackingRepository>,
    publisher: Option<Arc<dyn EventPublisher>>,
}

impl TrackingService {
    pub fn new(repo: Arc<dyn TrackingRepository>) -> Self {
        Self { repo, publisher: None }
    }

    pub fn with_publisher(mut self, publisher: Arc<dyn EventPublisher>) -> Self {
        self.publisher = Some(publisher);
        self
    }

    /// Public lookup — no auth; used for customer-facing tracking page.
    /// Returns only fields safe for public display (no driver phone, no tenant internals).
    pub async fn get_public(&self, tracking_number: &str) -> AppResult<TrackingRecord> {
        self.repo
            .find_by_tracking_number(tracking_number)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound {
                resource: "tracking",
                id: tracking_number.to_owned(),
            })
    }

    /// Authenticated lookup by shipment id, for a caller who may read it.
    ///
    /// A record the caller may not read is a 404, not a 403, so nobody can
    /// probe which shipment ids exist outside what they can see.
    pub async fn get_for(&self, shipment_id: Uuid, reader: &Reader) -> AppResult<TrackingRecord> {
        let not_found = || AppError::NotFound { resource: "shipment", id: shipment_id.to_string() };
        let record = self
            .repo
            .find_by_shipment_id(shipment_id)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(not_found)?;
        if may_read(reader, record.tenant_id.inner(), record.owner_id) {
            Ok(record)
        } else {
            Err(not_found())
        }
    }

    /// The caller's tenant, or only what they booked (authenticated, paginated).
    pub async fn list_for(&self, reader: &Reader, limit: i64, offset: i64) -> AppResult<Vec<TrackingRecord>> {
        let limit  = limit.clamp(1, 200);
        let offset = offset.max(0);
        self.repo
            .list_by_tenant(&TenantId::from_uuid(reader.tenant_id), reader.owner_filter(), limit, offset)
            .await
            .map_err(AppError::Internal)
    }

    /// Customer-initiated reschedule — verifies the tracking number exists, validates inputs, then delegates.
    pub async fn reschedule(
        &self,
        tracking_number: &str,
        preferred_date: chrono::NaiveDate,
        preferred_time_slot: Option<&str>,
        reason: &str,
    ) -> AppResult<()> {
        // Verify the tracking record exists first.
        self.repo
            .find_by_tracking_number(tracking_number)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound {
                resource: "tracking",
                id: tracking_number.to_owned(),
            })?;

        // Validate preferred_date: must be >= today (UTC).
        let today = chrono::Utc::now().date_naive();
        if preferred_date < today {
            return Err(AppError::Validation(
                "Preferred date must be in the future".to_owned(),
            ));
        }

        // Validate preferred_date: must be <= today + 90 days.
        let max_date = today + chrono::Duration::days(90);
        if preferred_date > max_date {
            return Err(AppError::Validation(
                "Preferred date cannot be more than 90 days in the future".to_owned(),
            ));
        }

        // Validate reason length if provided.
        if reason.len() > 500 {
            return Err(AppError::Validation(
                "Reason cannot exceed 500 characters".to_owned(),
            ));
        }

        self.repo
            .reschedule(tracking_number, preferred_date, preferred_time_slot, reason)
            .await
            .map_err(AppError::Internal)
    }

    /// Customer confirms they received their package.
    ///
    /// This is an optional, customer-initiated action that supplements the
    /// driver's POD capture.  The record is updated with
    /// `customer_confirmed_at`, and a `delivery.customer_confirmed` event is
    /// published downstream for analytics and engagement triggers.
    pub async fn confirm_customer_receipt(&self, tracking_number: &str) -> AppResult<()> {
        let record = self.repo
            .find_by_tracking_number(tracking_number)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound {
                resource: "tracking",
                id: tracking_number.to_owned(),
            })?;

        // Only allow confirmation when the shipment is delivered or out-for-delivery.
        use crate::domain::entities::TrackingStatus;
        match record.current_status {
            TrackingStatus::Delivered
            | TrackingStatus::OutForDelivery => {}
            _ => {
                return Err(AppError::BusinessRule(
                    "Receipt can only be confirmed for shipments that are out for delivery or already marked as delivered".into(),
                ));
            }
        }

        self.repo
            .confirm_customer_receipt(tracking_number)
            .await
            .map_err(AppError::Internal)
    }

    /// Customer requests their shipment receipt to be emailed.
    ///
    /// Persists the request to `tracking.receipt_email_requests` (audit trail
    /// and idempotency) and then publishes a `receipt.email.requested` Kafka
    /// event which engagement consumes to actually send the mail. A publish
    /// failure is logged but does not fail the request — the DB row remains
    /// as evidence and can be retried by a future poller.
    pub async fn send_receipt_email(
        &self,
        tracking_number: &str,
        email: &str,
    ) -> AppResult<()> {
        let email = email.trim();
        if email.is_empty() || !email.contains('@') {
            return Err(AppError::Validation("A valid email address is required".into()));
        }

        let record = self.repo
            .find_by_tracking_number(tracking_number)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound {
                resource: "tracking",
                id: tracking_number.to_owned(),
            })?;

        self.repo
            .record_receipt_email_request(tracking_number, email)
            .await
            .map_err(AppError::Internal)?;

        if let Some(publisher) = &self.publisher {
            use logisticos_events::{envelope::Event, payloads::ReceiptEmailRequested, topics};
            let payload = ReceiptEmailRequested {
                shipment_id:         record.shipment_id,
                tracking_number:     record.tracking_number.clone(),
                recipient_email:     email.to_owned(),
                origin_address:      record.origin_address.clone(),
                destination_address: record.destination_address.clone(),
                customer_id:         None,
                // recipient_name is populated by mark_delivered; fall back to empty
                // string — engagement renders "Customer" when absent.
                customer_name:       record.recipient_name.clone().unwrap_or_default(),
            };
            let event = Event::new(
                "logisticos/delivery-experience",
                "tracking.receipt.email.requested",
                record.tenant_id.inner(),
                payload,
            );
            match serde_json::to_string(&event) {
                Ok(json) => {
                    if let Err(e) = publisher
                        .publish(topics::RECEIPT_EMAIL_REQUESTED, tracking_number, &json)
                        .await
                    {
                        tracing::warn!(
                            tracking_number,
                            err = %e,
                            "send_receipt_email: Kafka publish failed — request persisted, email not dispatched"
                        );
                    }
                }
                Err(e) => {
                    tracing::warn!(tracking_number, err = %e, "send_receipt_email: event serialize failed");
                }
            }
        } else {
            tracing::warn!(
                tracking_number,
                "send_receipt_email: no publisher configured — email will not be dispatched"
            );
        }

        Ok(())
    }

    /// Public, unauthenticated feedback for a delivered shipment.
    ///
    /// The tracking number must resolve to a real record; tenant, shipment and
    /// driver are taken from it. Only one feedback per shipment is accepted
    /// (`Conflict`): on an unauthenticated route an upsert would let anyone
    /// holding the tracking number rewrite the customer's rating.
    pub async fn submit_feedback(
        &self,
        tracking_number: &str,
        rating: i16,
        tags: Vec<String>,
        comments: Option<String>,
    ) -> AppResult<()> {
        if !(1..=5).contains(&rating) {
            return Err(AppError::Validation("Rating must be between 1 and 5".to_owned()));
        }
        if tags.len() > FEEDBACK_MAX_TAGS || tags.iter().any(|t| t.chars().count() > FEEDBACK_MAX_TAG_CHARS) {
            return Err(AppError::Validation(format!(
                "At most {FEEDBACK_MAX_TAGS} tags of {FEEDBACK_MAX_TAG_CHARS} characters each"
            )));
        }
        if comments.as_deref().is_some_and(|c| c.chars().count() > FEEDBACK_MAX_COMMENT_CHARS) {
            return Err(AppError::Validation(format!(
                "Comments cannot exceed {FEEDBACK_MAX_COMMENT_CHARS} characters"
            )));
        }

        let record = self.repo
            .find_by_tracking_number(tracking_number)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound {
                resource: "tracking",
                id: tracking_number.to_owned(),
            })?;

        if !matches!(
            record.current_status,
            TrackingStatus::Delivered | TrackingStatus::DeliveryAttempted | TrackingStatus::DeliveryFailed
        ) {
            return Err(AppError::BusinessRule(
                "Feedback can only be left once a delivery has been attempted".to_owned(),
            ));
        }

        let saved = self.repo
            .save_feedback(&NewFeedback {
                tenant_id:       record.tenant_id.inner(),
                shipment_id:     record.shipment_id,
                driver_id:       record.driver_id,
                tracking_number: record.tracking_number.clone(),
                rating,
                tags,
                comments,
            })
            .await
            .map_err(AppError::Internal)?;
        if !saved {
            return Err(AppError::Conflict("Feedback has already been submitted for this shipment".to_owned()));
        }
        if let Some(driver_id) = record.driver_id {
            self.publish_feedback_submitted(record.tenant_id.inner(), record.shipment_id, driver_id, rating).await;
        }
        Ok(())
    }

    /// Tell driver-ops a driver was rated. Best-effort: the feedback row is
    /// already stored, so a broker failure is logged and never fails the
    /// customer's request.
    async fn publish_feedback_submitted(&self, tenant_id: Uuid, shipment_id: Uuid, driver_id: Uuid, rating: i16) {
        use logisticos_events::{envelope::Event, payloads::DeliveryFeedbackSubmitted, topics};
        let Some(publisher) = &self.publisher else {
            tracing::warn!(%shipment_id, "submit_feedback: no publisher configured — driver rating not propagated");
            return;
        };
        let event = Event::new(
            "logisticos/delivery-experience",
            "delivery.feedback.submitted",
            tenant_id,
            DeliveryFeedbackSubmitted { shipment_id, driver_id, rating },
        );
        let json = match serde_json::to_string(&event) {
            Ok(j) => j,
            Err(e) => {
                tracing::warn!(%shipment_id, err = %e, "submit_feedback: event serialize failed");
                return;
            }
        };
        if let Err(e) = publisher
            .publish(topics::DELIVERY_FEEDBACK_SUBMITTED, &tenant_id.to_string(), &json)
            .await
        {
            tracing::warn!(%shipment_id, %driver_id, err = %e,
                "submit_feedback: Kafka publish failed — feedback stored, driver rating not updated");
        }
    }
}
