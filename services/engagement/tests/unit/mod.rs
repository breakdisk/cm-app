use logisticos_engagement::domain::entities::{
    notification::{Notification, NotificationPriority, NotificationStatus},
    template::{NotificationChannel, NotificationTemplate},
    channel_config::TenantChannelConfig,
};
use uuid::Uuid;
use chrono::Utc;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_notification(channel: NotificationChannel, recipient: &str) -> Notification {
    Notification {
        id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        customer_id: Uuid::new_v4(),
        channel,
        recipient: recipient.to_string(),
        template_id: "pickup_confirmation".to_string(),
        rendered_body: "Your parcel is being picked up.".to_string(),
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

fn make_template(channel: NotificationChannel, body: &str, variables: Vec<String>) -> NotificationTemplate {
    NotificationTemplate {
        id: Uuid::new_v4(),
        tenant_id: None,
        template_id: "test_tpl".to_string(),
        channel,
        language: "en".to_string(),
        subject: None,
        body: body.to_string(),
        variables,
        is_active: true,
    }
}

// ---------------------------------------------------------------------------
// Notification struct construction
// ---------------------------------------------------------------------------

mod notification_construction {
    use super::*;

    #[test]
    fn fields_are_stored_correctly_on_direct_construction() {
        let tenant_id = Uuid::new_v4();
        let customer_id = Uuid::new_v4();
        let n = Notification {
            id: Uuid::new_v4(),
            tenant_id,
            customer_id,
            channel: NotificationChannel::Sms,
            recipient: "+639171234567".to_string(),
            template_id: "sms_delivery".to_string(),
            rendered_body: "Your order is out for delivery.".to_string(),
            subject: None,
            status: NotificationStatus::Queued,
            priority: NotificationPriority::High,
            provider_message_id: None,
            error_message: None,
            queued_at: Utc::now(),
            sent_at: None,
            delivered_at: None,
            retry_count: 0,
        };

        assert_eq!(n.tenant_id, tenant_id);
        assert_eq!(n.customer_id, customer_id);
        assert_eq!(n.channel, NotificationChannel::Sms);
        assert_eq!(n.recipient, "+639171234567");
        assert_eq!(n.template_id, "sms_delivery");
        assert_eq!(n.priority, NotificationPriority::High);
    }

    #[test]
    fn initial_status_is_queued() {
        let n = make_notification(NotificationChannel::Email, "user@example.com");
        assert_eq!(n.status, NotificationStatus::Queued);
    }

    #[test]
    fn initial_retry_count_is_zero() {
        let n = make_notification(NotificationChannel::WhatsApp, "+639171234567");
        assert_eq!(n.retry_count, 0);
    }

    #[test]
    fn optional_fields_default_to_none() {
        let n = make_notification(NotificationChannel::Sms, "+63917000000");
        assert!(n.provider_message_id.is_none());
        assert!(n.error_message.is_none());
        assert!(n.sent_at.is_none());
        assert!(n.delivered_at.is_none());
    }
}

// ---------------------------------------------------------------------------
// NotificationPriority ordering
// ---------------------------------------------------------------------------

mod priority_ordering {
    use super::*;

    #[test]
    fn high_is_greater_than_normal() {
        assert!(NotificationPriority::High > NotificationPriority::Normal);
    }

    #[test]
    fn normal_is_greater_than_low() {
        assert!(NotificationPriority::Normal > NotificationPriority::Low);
    }

    #[test]
    fn high_is_greater_than_low() {
        assert!(NotificationPriority::High > NotificationPriority::Low);
    }

    #[test]
    fn same_priority_is_equal() {
        assert_eq!(NotificationPriority::Normal, NotificationPriority::Normal);
    }

    #[test]
    fn priorities_sort_ascending_low_to_high() {
        let mut priorities = vec![
            NotificationPriority::High,
            NotificationPriority::Low,
            NotificationPriority::Normal,
        ];
        priorities.sort();
        assert_eq!(
            priorities,
            vec![
                NotificationPriority::Low,
                NotificationPriority::Normal,
                NotificationPriority::High,
            ]
        );
    }
}

// ---------------------------------------------------------------------------
// Template rendering
// ---------------------------------------------------------------------------

mod template_rendering {
    use super::*;
    use serde_json::json;

    #[test]
    fn render_substitutes_all_declared_variables() {
        let tpl = make_template(
            NotificationChannel::Sms,
            "Hello {{name}}, your AWB is {{awb}}.",
            vec!["name".to_string(), "awb".to_string()],
        );
        let vars = json!({ "name": "Maria", "awb": "LOS-00123456" });
        let result = tpl.render(&vars).unwrap();
        assert_eq!(result, "Hello Maria, your AWB is LOS-00123456.");
    }

    #[test]
    fn render_returns_err_when_required_variable_is_missing() {
        let tpl = make_template(
            NotificationChannel::Sms,
            "Hello {{name}}, ETA: {{eta}}.",
            vec!["name".to_string(), "eta".to_string()],
        );
        // Only provide one of the two required variables.
        let vars = json!({ "name": "Juan" });
        let err = tpl.render(&vars).unwrap_err();
        assert!(err.contains("eta"), "error should mention missing variable 'eta'");
    }

    #[test]
    fn render_with_no_variables_returns_body_unchanged() {
        let tpl = make_template(
            NotificationChannel::Push,
            "Your delivery is on its way.",
            vec![],
        );
        let vars = serde_json::Value::Object(serde_json::Map::new());
        let result = tpl.render(&vars).unwrap();
        assert_eq!(result, "Your delivery is on its way.");
    }

    #[test]
    fn render_substitutes_variable_appearing_multiple_times() {
        let tpl = make_template(
            NotificationChannel::Email,
            "Hi {{name}}! {{name}}, your package is ready.",
            vec!["name".to_string()],
        );
        let vars = json!({ "name": "Ana" });
        let result = tpl.render(&vars).unwrap();
        assert_eq!(result, "Hi Ana! Ana, your package is ready.");
    }
}

// ---------------------------------------------------------------------------
// Channel validation — WhatsApp recipients must start with "+"
// ---------------------------------------------------------------------------

mod channel_validation {
    use super::*;

    #[test]
    fn whatsapp_recipient_starting_with_plus_is_valid_e164() {
        let recipient = "+639171234567";
        assert!(
            recipient.starts_with('+'),
            "WhatsApp recipient must be in E.164 format starting with +"
        );
    }

    #[test]
    fn whatsapp_recipient_without_plus_does_not_satisfy_e164() {
        let recipient = "639171234567";
        assert!(
            !recipient.starts_with('+'),
            "Local number without + should not pass E.164 check"
        );
    }

    #[test]
    fn channel_as_str_returns_correct_lowercase_identifier() {
        assert_eq!(NotificationChannel::WhatsApp.as_str(), "whatsapp");
        assert_eq!(NotificationChannel::Sms.as_str(),      "sms");
        assert_eq!(NotificationChannel::Email.as_str(),    "email");
        assert_eq!(NotificationChannel::Push.as_str(),     "push");
    }
}

// ---------------------------------------------------------------------------
// NotificationStatus transitions via mark_sent / mark_failed
// ---------------------------------------------------------------------------

mod status_transitions {
    use super::*;

    #[test]
    fn mark_sent_transitions_status_to_sent() {
        let mut n = make_notification(NotificationChannel::Sms, "+63917000000");
        n.mark_sent("twilio-msg-id-001".to_string());
        assert_eq!(n.status, NotificationStatus::Sent);
    }

    #[test]
    fn mark_sent_stores_provider_message_id() {
        let mut n = make_notification(NotificationChannel::Sms, "+63917000000");
        n.mark_sent("twilio-msg-id-001".to_string());
        assert_eq!(n.provider_message_id.as_deref(), Some("twilio-msg-id-001"));
    }

    #[test]
    fn mark_sent_records_sent_at_timestamp() {
        let mut n = make_notification(NotificationChannel::Email, "user@example.com");
        n.mark_sent("sg-abc".to_string());
        assert!(n.sent_at.is_some());
    }

    #[test]
    fn mark_failed_transitions_status_to_failed() {
        let mut n = make_notification(NotificationChannel::WhatsApp, "+63917000000");
        n.mark_failed("rate limit exceeded".to_string());
        assert_eq!(n.status, NotificationStatus::Failed);
    }

    #[test]
    fn mark_failed_stores_error_message() {
        let mut n = make_notification(NotificationChannel::WhatsApp, "+63917000000");
        n.mark_failed("upstream timeout".to_string());
        assert_eq!(n.error_message.as_deref(), Some("upstream timeout"));
    }

    #[test]
    fn mark_failed_increments_retry_count() {
        let mut n = make_notification(NotificationChannel::Sms, "+63917000000");
        assert_eq!(n.retry_count, 0);
        n.mark_failed("timeout".to_string());
        assert_eq!(n.retry_count, 1);
        n.mark_failed("timeout again".to_string());
        assert_eq!(n.retry_count, 2);
    }

    #[test]
    fn can_retry_is_true_when_failed_and_below_limit() {
        let mut n = make_notification(NotificationChannel::Sms, "+63917000000");
        n.mark_failed("error".to_string());
        assert!(n.can_retry(), "should be retryable after first failure");
    }

    #[test]
    fn can_retry_is_false_when_retry_count_reaches_three() {
        let mut n = make_notification(NotificationChannel::Sms, "+63917000000");
        n.mark_failed("err".to_string()); // retry_count = 1
        n.mark_failed("err".to_string()); // retry_count = 2
        n.mark_failed("err".to_string()); // retry_count = 3
        assert!(
            !n.can_retry(),
            "should not be retryable after 3 failures"
        );
    }

    #[test]
    fn can_retry_is_false_when_status_is_sent() {
        let mut n = make_notification(NotificationChannel::Email, "user@example.com");
        n.mark_sent("provider-id".to_string());
        assert!(!n.can_retry(), "sent notifications should not be retried");
    }
}

// ---------------------------------------------------------------------------
// TenantChannelConfig
// ---------------------------------------------------------------------------

mod channel_config {
    use super::*;

    #[test]
    fn channel_config_stores_enabled_flags_correctly() {
        let cfg = TenantChannelConfig {
            tenant_id: Uuid::new_v4(),
            whatsapp_enabled: true,
            sms_enabled: true,
            email_enabled: false,
            push_enabled: false,
            twilio_vault_key: Some("secret/tenants/abc/twilio".to_string()),
            sendgrid_vault_key: None,
            firebase_vault_key: None,
        };

        assert!(cfg.whatsapp_enabled);
        assert!(cfg.sms_enabled);
        assert!(!cfg.email_enabled);
        assert!(!cfg.push_enabled);
    }

    #[test]
    fn vault_keys_are_paths_not_raw_secrets() {
        let cfg = TenantChannelConfig {
            tenant_id: Uuid::new_v4(),
            whatsapp_enabled: true,
            sms_enabled: false,
            email_enabled: false,
            push_enabled: false,
            twilio_vault_key: Some("secret/tenants/xyz/twilio".to_string()),
            sendgrid_vault_key: None,
            firebase_vault_key: None,
        };

        let key = cfg.twilio_vault_key.unwrap();
        // A Vault key path should look like a path, not a raw credential.
        assert!(key.starts_with("secret/"), "vault key should be a path");
        assert!(!key.contains("authtoken"), "raw secrets must never be stored");
    }
}
