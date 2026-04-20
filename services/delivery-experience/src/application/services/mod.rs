use std::sync::Arc;
use uuid::Uuid;

use logisticos_errors::{AppError, AppResult};
use logisticos_types::TenantId;

use crate::domain::{entities::TrackingRecord, repositories::TrackingRepository};

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
            .map_err(|e| AppError::Internal(e))?
            .ok_or_else(|| AppError::NotFound {
                resource: "tracking",
                id: tracking_number.to_owned(),
            })
    }

    /// Authenticated merchant lookup by shipment id.
    pub async fn get_by_shipment_id(&self, shipment_id: Uuid) -> AppResult<TrackingRecord> {
        self.repo
            .find_by_shipment_id(shipment_id)
            .await
            .map_err(|e| AppError::Internal(e))?
            .ok_or_else(|| AppError::NotFound {
                resource: "shipment",
                id: shipment_id.to_string(),
            })
    }

    /// List shipments for a tenant (authenticated, paginated).
    pub async fn list(&self, tenant_id: &TenantId, limit: i64, offset: i64) -> AppResult<Vec<TrackingRecord>> {
        let limit  = limit.clamp(1, 200);
        let offset = offset.max(0);
        self.repo
            .list_by_tenant(tenant_id, limit, offset)
            .await
            .map_err(|e| AppError::Internal(e))
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
            .map_err(|e| AppError::Internal(e))?
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
            .map_err(|e| AppError::Internal(e))
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
            .map_err(|e| AppError::Internal(e))?
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
            .map_err(|e| AppError::Internal(e))
    }

    /// Customer requests their shipment receipt to be emailed.
    ///
    /// Persists the request to `tracking.receipt_email_requests` (audit trail
    /// + idempotency) and then publishes a `receipt.email.requested` Kafka
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
            .map_err(|e| AppError::Internal(e))?
            .ok_or_else(|| AppError::NotFound {
                resource: "tracking",
                id: tracking_number.to_owned(),
            })?;

        self.repo
            .record_receipt_email_request(tracking_number, email)
            .await
            .map_err(|e| AppError::Internal(e))?;

        if let Some(publisher) = &self.publisher {
            use logisticos_events::{envelope::Event, payloads::ReceiptEmailRequested, topics};
            let payload = ReceiptEmailRequested {
                shipment_id:         record.shipment_id,
                tracking_number:     record.tracking_number.clone(),
                recipient_email:     email.to_owned(),
                origin_address:      record.origin_address.clone(),
                destination_address: record.destination_address.clone(),
                customer_id:         None,
                customer_name:       String::new(),
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

    pub async fn submit_feedback(
        &self,
        tracking_number: &str,
        rating: i16,
        tags: Vec<String>,
        comments: Option<String>,
    ) -> AppResult<()> {
        // Validate rating
        if !(1..=5).contains(&rating) {
            return Err(AppError::Validation("Rating must be between 1 and 5".to_owned()));
        }
        self.repo
            .save_feedback(tracking_number, rating, &tags, comments.as_deref())
            .await
            .map_err(|e| AppError::Internal(e))
    }
}
