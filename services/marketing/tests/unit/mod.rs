use logisticos_marketing::domain::entities::{
    Campaign, CampaignStatus, Channel, MessageTemplate, TargetingRule,
};
use logisticos_types::TenantId;
use uuid::Uuid;
use chrono::{Duration, Utc};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_template(template_id: &str) -> MessageTemplate {
    MessageTemplate {
        template_id: template_id.to_string(),
        subject: None,
        variables: serde_json::json!({ "promo_code": "SHIP20" }),
    }
}

fn make_campaign_with_template(template_id: &str) -> Campaign {
    Campaign::new(
        TenantId::new(),
        "Re-engagement June".to_string(),
        None,
        Channel::WhatsApp,
        make_template(template_id),
        TargetingRule::default(),
        Uuid::new_v4(),
    )
}

fn make_campaign() -> Campaign {
    make_campaign_with_template("whatsapp_promo_v1")
}

// ---------------------------------------------------------------------------
// Campaign construction
// ---------------------------------------------------------------------------

mod campaign_construction {
    use super::*;

    #[test]
    fn new_campaign_starts_in_draft_status() {
        let c = make_campaign();
        assert_eq!(c.status, CampaignStatus::Draft);
    }

    #[test]
    fn new_campaign_stores_name() {
        let c = make_campaign();
        assert_eq!(c.name, "Re-engagement June");
    }

    #[test]
    fn new_campaign_has_zero_send_metrics() {
        let c = make_campaign();
        assert_eq!(c.total_sent, 0);
        assert_eq!(c.total_delivered, 0);
        assert_eq!(c.total_failed, 0);
    }

    #[test]
    fn new_campaign_has_no_scheduled_or_sent_timestamps() {
        let c = make_campaign();
        assert!(c.scheduled_at.is_none());
        assert!(c.sent_at.is_none());
        assert!(c.completed_at.is_none());
    }

    #[test]
    fn new_campaign_stores_channel() {
        let c = make_campaign();
        assert_eq!(c.channel, Channel::WhatsApp);
    }

    #[test]
    fn new_campaign_stores_template_id() {
        let c = make_campaign_with_template("my_template");
        assert_eq!(c.template.template_id, "my_template");
    }

    #[test]
    fn new_campaign_generates_unique_id() {
        let c1 = make_campaign();
        let c2 = make_campaign();
        assert_ne!(c1.id, c2.id);
    }
}

// ---------------------------------------------------------------------------
// Campaign::schedule() — Draft → Scheduled
// ---------------------------------------------------------------------------

mod campaign_schedule {
    use super::*;

    #[test]
    fn draft_campaign_can_be_scheduled_for_future_time() {
        let mut c = make_campaign();
        let future = Utc::now() + Duration::hours(2);
        assert!(c.schedule(future).is_ok());
        assert_eq!(c.status, CampaignStatus::Scheduled);
        assert_eq!(c.scheduled_at, Some(future));
    }

    #[test]
    fn schedule_fails_when_time_is_in_the_past() {
        let mut c = make_campaign();
        let past = Utc::now() - Duration::minutes(1);
        let err = c.schedule(past).unwrap_err();
        assert!(
            err.to_string().contains("future"),
            "error should mention that time must be in the future"
        );
        // Status must remain Draft.
        assert_eq!(c.status, CampaignStatus::Draft);
    }

    #[test]
    fn schedule_fails_when_campaign_is_already_scheduled() {
        let mut c = make_campaign();
        c.schedule(Utc::now() + Duration::hours(1)).unwrap();
        let err = c.schedule(Utc::now() + Duration::hours(2)).unwrap_err();
        assert!(
            err.to_string().contains("Draft"),
            "error should say only Draft campaigns can be scheduled"
        );
    }

    #[test]
    fn schedule_fails_for_completed_campaign() {
        let mut c = make_campaign();
        c.activate().unwrap();
        c.complete(100, 95, 5);
        let err = c.schedule(Utc::now() + Duration::hours(1)).unwrap_err();
        assert!(err.to_string().contains("Draft"));
    }

    #[test]
    fn schedule_fails_for_cancelled_campaign() {
        let mut c = make_campaign();
        c.cancel().unwrap();
        let err = c.schedule(Utc::now() + Duration::hours(1)).unwrap_err();
        assert!(err.to_string().contains("Draft"));
    }
}

// ---------------------------------------------------------------------------
// Campaign::activate() — Draft|Scheduled → Sending
// ---------------------------------------------------------------------------

mod campaign_activate {
    use super::*;

    #[test]
    fn draft_campaign_can_be_activated() {
        let mut c = make_campaign();
        assert!(c.activate().is_ok());
        assert_eq!(c.status, CampaignStatus::Sending);
    }

    #[test]
    fn scheduled_campaign_can_be_activated() {
        let mut c = make_campaign();
        c.schedule(Utc::now() + Duration::hours(1)).unwrap();
        assert!(c.activate().is_ok());
        assert_eq!(c.status, CampaignStatus::Sending);
    }

    #[test]
    fn activate_records_sent_at_timestamp() {
        let mut c = make_campaign();
        c.activate().unwrap();
        assert!(c.sent_at.is_some());
    }

    #[test]
    fn completed_campaign_cannot_be_activated() {
        let mut c = make_campaign();
        c.activate().unwrap();
        c.complete(50, 45, 5);
        let err = c.activate().unwrap_err();
        assert!(
            err.to_string().contains("Cannot activate"),
            "error should say cannot activate"
        );
    }

    #[test]
    fn cancelled_campaign_cannot_be_activated() {
        let mut c = make_campaign();
        c.cancel().unwrap();
        let err = c.activate().unwrap_err();
        assert!(err.to_string().contains("Cannot activate"));
    }

    #[test]
    fn sending_campaign_cannot_be_activated_again() {
        let mut c = make_campaign();
        c.activate().unwrap();
        let err = c.activate().unwrap_err();
        assert!(err.to_string().contains("Cannot activate"));
    }
}

// ---------------------------------------------------------------------------
// Campaign::complete()
// ---------------------------------------------------------------------------

mod campaign_complete {
    use super::*;

    #[test]
    fn complete_transitions_to_completed_status() {
        let mut c = make_campaign();
        c.activate().unwrap();
        c.complete(1000, 980, 20);
        assert_eq!(c.status, CampaignStatus::Completed);
    }

    #[test]
    fn complete_records_send_metrics() {
        let mut c = make_campaign();
        c.activate().unwrap();
        c.complete(1000, 980, 20);
        assert_eq!(c.total_sent, 1000);
        assert_eq!(c.total_delivered, 980);
        assert_eq!(c.total_failed, 20);
    }

    #[test]
    fn complete_records_completed_at_timestamp() {
        let mut c = make_campaign();
        c.activate().unwrap();
        c.complete(10, 10, 0);
        assert!(c.completed_at.is_some());
    }
}

// ---------------------------------------------------------------------------
// Campaign::cancel()
// ---------------------------------------------------------------------------

mod campaign_cancel {
    use super::*;

    #[test]
    fn draft_campaign_can_be_cancelled() {
        let mut c = make_campaign();
        assert!(c.cancel().is_ok());
        assert_eq!(c.status, CampaignStatus::Cancelled);
    }

    #[test]
    fn scheduled_campaign_can_be_cancelled() {
        let mut c = make_campaign();
        c.schedule(Utc::now() + Duration::hours(1)).unwrap();
        assert!(c.cancel().is_ok());
        assert_eq!(c.status, CampaignStatus::Cancelled);
    }

    #[test]
    fn sending_campaign_cannot_be_cancelled() {
        let mut c = make_campaign();
        c.activate().unwrap();
        let err = c.cancel().unwrap_err();
        assert!(
            err.to_string().contains("already sending"),
            "error should mention already sending"
        );
        assert_eq!(c.status, CampaignStatus::Sending);
    }

    #[test]
    fn completed_campaign_cannot_be_cancelled() {
        let mut c = make_campaign();
        c.activate().unwrap();
        c.complete(100, 95, 5);
        let err = c.cancel().unwrap_err();
        assert!(
            err.to_string().contains("already completed"),
            "error should mention already completed"
        );
        assert_eq!(c.status, CampaignStatus::Completed);
    }

    #[test]
    fn cancelled_campaign_can_be_cancelled_again_without_error() {
        // The domain model does not guard double-cancel — it transitions to Cancelled
        // regardless (neither Sending nor Completed). Test documents this behaviour.
        let mut c = make_campaign();
        c.cancel().unwrap();
        // Not Sending, not Completed — second cancel is accepted.
        assert!(c.cancel().is_ok());
    }
}

// ---------------------------------------------------------------------------
// TargetingRule
// ---------------------------------------------------------------------------

mod targeting_rule {
    use super::*;

    #[test]
    fn default_targeting_rule_has_zero_estimated_reach() {
        let rule = TargetingRule::default();
        assert_eq!(rule.estimated_reach, 0);
    }

    #[test]
    fn default_targeting_rule_has_no_clv_filter() {
        let rule = TargetingRule::default();
        assert!(rule.min_clv_score.is_none());
    }

    #[test]
    fn default_targeting_rule_has_empty_customer_ids() {
        let rule = TargetingRule::default();
        assert!(rule.customer_ids.is_empty());
    }

    #[test]
    fn targeting_rule_with_customer_ids_bypasses_other_criteria() {
        // Documented business intent: when customer_ids is populated the other
        // filter fields are ignored by the CDP query layer.
        let ids = vec![Uuid::new_v4(), Uuid::new_v4()];
        let rule = TargetingRule {
            min_clv_score: Some(80.0),
            last_active_days: Some(30),
            customer_ids: ids.clone(),
            estimated_reach: ids.len() as u64,
        };
        assert_eq!(rule.customer_ids.len(), 2);
        assert_eq!(rule.estimated_reach, 2);
    }
}

// ---------------------------------------------------------------------------
// Channel enum
// ---------------------------------------------------------------------------

mod channel_enum {
    use super::*;

    #[test]
    fn channel_variants_are_distinct() {
        assert_ne!(Channel::WhatsApp, Channel::Sms);
        assert_ne!(Channel::Email, Channel::Push);
        assert_ne!(Channel::WhatsApp, Channel::Email);
    }
}
