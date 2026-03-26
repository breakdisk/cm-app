//! Integration tests for the Engagement service's NotificationService layer.
//!
//! The engagement service is primarily event-driven, but it also exposes an
//! HTTP API. These tests exercise the `NotificationService::dispatch()` method
//! directly using mock `ChannelAdapter` implementations that record the calls
//! they receive — no real Twilio/SendGrid/WhatsApp credentials are needed.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;
use uuid::Uuid;
use chrono::Utc;

use logisticos_engagement::{
    application::services::notification_service::NotificationService,
    domain::entities::{
        notification::{Notification, NotificationPriority, NotificationStatus},
        template::{NotificationChannel, NotificationTemplate},
    },
    infrastructure::channels::ChannelAdapter,
};

// ---------------------------------------------------------------------------
// Mock channel adapter
// ---------------------------------------------------------------------------

/// Tracks every `send` call and allows the test to configure a fixed outcome.
struct MockAdapter {
    /// Calls recorded as (recipient, body, subject).
    calls: Mutex<Vec<(String, String, Option<String>)>>,
    /// When `Some(msg)`, the adapter returns an error with this message.
    fail_with: Option<String>,
    /// Provider message ID returned on success.
    provider_id: String,
}

impl MockAdapter {
    fn succeeds(provider_id: &str) -> Arc<Self> {
        Arc::new(Self {
            calls: Mutex::new(Vec::new()),
            fail_with: None,
            provider_id: provider_id.to_string(),
        })
    }

    fn fails(error: &str) -> Arc<Self> {
        Arc::new(Self {
            calls: Mutex::new(Vec::new()),
            fail_with: Some(error.to_string()),
            provider_id: String::new(),
        })
    }

    async fn call_count(&self) -> usize {
        self.calls.lock().await.len()
    }

    async fn last_recipient(&self) -> Option<String> {
        self.calls.lock().await.last().map(|(r, _, _)| r.clone())
    }
}

#[async_trait]
impl ChannelAdapter for MockAdapter {
    async fn send(&self, recipient: &str, body: &str, subject: Option<&str>) -> Result<String, String> {
        self.calls.lock().await.push((
            recipient.to_string(),
            body.to_string(),
            subject.map(|s| s.to_string()),
        ));
        if let Some(err) = &self.fail_with {
            Err(err.clone())
        } else {
            Ok(self.provider_id.clone())
        }
    }
}

// ---------------------------------------------------------------------------
// Notification builder helper
// ---------------------------------------------------------------------------

fn make_notification(channel: NotificationChannel, recipient: &str) -> Notification {
    Notification {
        id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        customer_id: Uuid::new_v4(),
        channel,
        recipient: recipient.to_string(),
        template_id: "test_tpl".to_string(),
        rendered_body: "Your parcel will arrive today.".to_string(),
        subject: None,
        status: NotificationStatus::Queued,
        priority: NotificationPriority::Normal,
        provider_message_id: None,
        error_message: None,
        queued_at: Utc::now(),
        sent_at: None,
        delivered_at: None,
        retry_count: 0,
    }
}

fn make_template(channel: NotificationChannel, body: &str, vars: Vec<String>) -> NotificationTemplate {
    NotificationTemplate {
        id: Uuid::new_v4(),
        tenant_id: None,
        template_id: "tpl_001".to_string(),
        channel,
        language: "en".to_string(),
        subject: None,
        body: body.to_string(),
        variables: vars,
        is_active: true,
    }
}

// ---------------------------------------------------------------------------
// Routing: dispatch() sends to the correct channel adapter
// ---------------------------------------------------------------------------

mod dispatch_routing {
    use super::*;

    #[tokio::test]
    async fn whatsapp_notification_is_routed_to_whatsapp_adapter() {
        let whatsapp = MockAdapter::succeeds("wa-msg-001");
        let sms      = MockAdapter::succeeds("sms-msg-001");
        let email    = MockAdapter::succeeds("email-msg-001");

        let svc = NotificationService::new(whatsapp.clone(), sms.clone(), email.clone());
        let mut n = make_notification(NotificationChannel::WhatsApp, "+639171234567");
        svc.dispatch(&mut n).await.unwrap();

        assert_eq!(whatsapp.call_count().await, 1, "WhatsApp adapter should have been called once");
        assert_eq!(sms.call_count().await,      0, "SMS adapter must not be called");
        assert_eq!(email.call_count().await,    0, "Email adapter must not be called");
    }

    #[tokio::test]
    async fn sms_notification_is_routed_to_sms_adapter() {
        let whatsapp = MockAdapter::succeeds("wa");
        let sms      = MockAdapter::succeeds("sms-abc");
        let email    = MockAdapter::succeeds("em");

        let svc = NotificationService::new(whatsapp.clone(), sms.clone(), email.clone());
        let mut n = make_notification(NotificationChannel::Sms, "+639170000002");
        svc.dispatch(&mut n).await.unwrap();

        assert_eq!(sms.call_count().await,      1);
        assert_eq!(whatsapp.call_count().await, 0);
        assert_eq!(email.call_count().await,    0);
    }

    #[tokio::test]
    async fn email_notification_is_routed_to_email_adapter() {
        let whatsapp = MockAdapter::succeeds("wa");
        let sms      = MockAdapter::succeeds("sms");
        let email    = MockAdapter::succeeds("sg-abc123");

        let svc = NotificationService::new(whatsapp.clone(), sms.clone(), email.clone());
        let mut n = make_notification(NotificationChannel::Email, "customer@example.com");
        svc.dispatch(&mut n).await.unwrap();

        assert_eq!(email.call_count().await,    1);
        assert_eq!(whatsapp.call_count().await, 0);
        assert_eq!(sms.call_count().await,      0);
    }

    #[tokio::test]
    async fn adapter_receives_correct_recipient() {
        let whatsapp = MockAdapter::succeeds("wa-id");
        let svc = NotificationService::new(
            whatsapp.clone(),
            MockAdapter::succeeds("s"),
            MockAdapter::succeeds("e"),
        );
        let mut n = make_notification(NotificationChannel::WhatsApp, "+6391799999");
        svc.dispatch(&mut n).await.unwrap();

        assert_eq!(
            whatsapp.last_recipient().await.as_deref(),
            Some("+6391799999")
        );
    }

    #[tokio::test]
    async fn push_notification_returns_error_because_push_is_not_wired() {
        let svc = NotificationService::new(
            MockAdapter::succeeds("wa"),
            MockAdapter::succeeds("sms"),
            MockAdapter::succeeds("email"),
        );
        let mut n = make_notification(NotificationChannel::Push, "device-token-abc");
        let result = svc.dispatch(&mut n).await;
        assert!(result.is_err(), "Push should return an error until wired");
    }
}

// ---------------------------------------------------------------------------
// On success: notification transitions to Sent
// ---------------------------------------------------------------------------

mod dispatch_success {
    use super::*;

    #[tokio::test]
    async fn successful_dispatch_marks_notification_as_sent() {
        let svc = NotificationService::new(
            MockAdapter::succeeds("wa-provider-001"),
            MockAdapter::succeeds("sms"),
            MockAdapter::succeeds("email"),
        );
        let mut n = make_notification(NotificationChannel::WhatsApp, "+63917000001");
        svc.dispatch(&mut n).await.unwrap();

        assert_eq!(n.status, NotificationStatus::Sent);
    }

    #[tokio::test]
    async fn successful_dispatch_stores_provider_message_id() {
        let svc = NotificationService::new(
            MockAdapter::succeeds("wa-provider-001"),
            MockAdapter::succeeds("sms"),
            MockAdapter::succeeds("email"),
        );
        let mut n = make_notification(NotificationChannel::WhatsApp, "+63917000001");
        svc.dispatch(&mut n).await.unwrap();

        assert_eq!(
            n.provider_message_id.as_deref(),
            Some("wa-provider-001")
        );
    }

    #[tokio::test]
    async fn successful_dispatch_records_sent_at_timestamp() {
        let svc = NotificationService::new(
            MockAdapter::succeeds("wa-001"),
            MockAdapter::succeeds("s"),
            MockAdapter::succeeds("e"),
        );
        let mut n = make_notification(NotificationChannel::WhatsApp, "+63917000001");
        svc.dispatch(&mut n).await.unwrap();

        assert!(n.sent_at.is_some());
    }
}

// ---------------------------------------------------------------------------
// On failure: notification transitions to Failed and can_retry is set
// ---------------------------------------------------------------------------

mod dispatch_failure {
    use super::*;

    #[tokio::test]
    async fn failed_adapter_marks_notification_as_failed() {
        let svc = NotificationService::new(
            MockAdapter::fails("upstream timeout"),
            MockAdapter::succeeds("sms"),
            MockAdapter::succeeds("email"),
        );
        let mut n = make_notification(NotificationChannel::WhatsApp, "+63917000001");
        // First failure — retry_count becomes 1, still can_retry → dispatch returns Ok.
        svc.dispatch(&mut n).await.unwrap();

        assert_eq!(n.status, NotificationStatus::Failed);
    }

    #[tokio::test]
    async fn failed_adapter_stores_error_message() {
        let svc = NotificationService::new(
            MockAdapter::fails("rate limit exceeded"),
            MockAdapter::succeeds("sms"),
            MockAdapter::succeeds("email"),
        );
        let mut n = make_notification(NotificationChannel::WhatsApp, "+63917000001");
        svc.dispatch(&mut n).await.unwrap();

        assert_eq!(
            n.error_message.as_deref(),
            Some("rate limit exceeded")
        );
    }

    #[tokio::test]
    async fn failed_adapter_increments_retry_count() {
        let svc = NotificationService::new(
            MockAdapter::fails("error"),
            MockAdapter::succeeds("sms"),
            MockAdapter::succeeds("email"),
        );
        let mut n = make_notification(NotificationChannel::WhatsApp, "+63917000001");
        svc.dispatch(&mut n).await.unwrap();

        assert_eq!(n.retry_count, 1);
    }

    #[tokio::test]
    async fn notification_at_retry_limit_causes_dispatch_to_return_err() {
        let svc = NotificationService::new(
            MockAdapter::fails("persistent error"),
            MockAdapter::succeeds("sms"),
            MockAdapter::succeeds("email"),
        );
        let mut n = make_notification(NotificationChannel::WhatsApp, "+63917000001");

        // Exhaust retry budget: 3 failures means retry_count == 3, can_retry() == false.
        // dispatch() returns Ok for failures that are still retryable, Err when exhausted.
        n.mark_failed("pre-existing error 1".to_string()); // retry_count = 1
        n.mark_failed("pre-existing error 2".to_string()); // retry_count = 2
        // Now retry_count == 2; one more failure will bring it to 3 → can_retry() = false → Err.
        let result = svc.dispatch(&mut n).await;
        assert!(result.is_err(), "should return Err when retry limit is reached");
    }

    #[tokio::test]
    async fn notification_that_can_retry_dispatch_returns_ok_not_err() {
        let svc = NotificationService::new(
            MockAdapter::fails("transient error"),
            MockAdapter::succeeds("sms"),
            MockAdapter::succeeds("email"),
        );
        let mut n = make_notification(NotificationChannel::WhatsApp, "+63917000001");
        // retry_count is 0 — first failure is recoverable.
        let result = svc.dispatch(&mut n).await;
        assert!(
            result.is_ok(),
            "first failure should not bubble as Err while retries remain"
        );
    }
}

// ---------------------------------------------------------------------------
// build_from_template: constructs a well-formed Notification
// ---------------------------------------------------------------------------

mod build_from_template {
    use super::*;
    use serde_json::json;

    #[test]
    fn builds_notification_with_queued_status() {
        let tpl = make_template(
            NotificationChannel::Sms,
            "Hi {{name}}, your order is ready.",
            vec!["name".to_string()],
        );
        let vars = json!({ "name": "Juana" });
        let n = NotificationService::build_from_template(
            &tpl,
            Uuid::new_v4(),
            Uuid::new_v4(),
            "+639170000099".to_string(),
            &vars,
            NotificationPriority::Normal,
        ).unwrap();

        assert_eq!(n.status, NotificationStatus::Queued);
    }

    #[test]
    fn builds_notification_with_rendered_body() {
        let tpl = make_template(
            NotificationChannel::Sms,
            "Hi {{name}}, AWB {{awb}}.",
            vec!["name".to_string(), "awb".to_string()],
        );
        let vars = json!({ "name": "Pedro", "awb": "LOS-00042" });
        let n = NotificationService::build_from_template(
            &tpl, Uuid::new_v4(), Uuid::new_v4(),
            "+63917000000".to_string(), &vars, NotificationPriority::Normal,
        ).unwrap();

        assert_eq!(n.rendered_body, "Hi Pedro, AWB LOS-00042.");
    }

    #[test]
    fn returns_err_when_required_template_variable_is_missing() {
        let tpl = make_template(
            NotificationChannel::Sms,
            "Hello {{name}}.",
            vec!["name".to_string()],
        );
        let vars = serde_json::Value::Object(serde_json::Map::new()); // empty
        let result = NotificationService::build_from_template(
            &tpl, Uuid::new_v4(), Uuid::new_v4(),
            "+63917000000".to_string(), &vars, NotificationPriority::Normal,
        );
        assert!(result.is_err());
    }

    #[test]
    fn builds_notification_with_correct_channel_from_template() {
        let tpl = make_template(NotificationChannel::Email, "Welcome!", vec![]);
        let vars = serde_json::Value::Object(serde_json::Map::new());
        let n = NotificationService::build_from_template(
            &tpl, Uuid::new_v4(), Uuid::new_v4(),
            "user@example.com".to_string(), &vars, NotificationPriority::High,
        ).unwrap();

        assert_eq!(n.channel, NotificationChannel::Email);
        assert_eq!(n.priority, NotificationPriority::High);
    }

    #[test]
    fn builds_notification_with_zero_retry_count() {
        let tpl = make_template(NotificationChannel::Sms, "Test.", vec![]);
        let vars = serde_json::Value::Object(serde_json::Map::new());
        let n = NotificationService::build_from_template(
            &tpl, Uuid::new_v4(), Uuid::new_v4(),
            "+63917000000".to_string(), &vars, NotificationPriority::Low,
        ).unwrap();

        assert_eq!(n.retry_count, 0);
    }
}
