use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

use logisticos_ai_layer::domain::entities::{
    AgentAction, AgentMessage, AgentSession, AgentType, MessageRole, SessionStatus,
};
use logisticos_types::TenantId;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_session(agent_type: AgentType) -> AgentSession {
    AgentSession::new(TenantId::new(), agent_type, json!({"source": "test"}))
}

fn make_message(role: MessageRole) -> AgentMessage {
    AgentMessage {
        role,
        content: serde_json::Value::String("test message".into()),
    }
}

fn make_action(session_id: Uuid, tool: &str) -> AgentAction {
    AgentAction::new(session_id, tool.into(), json!({"param": "value"}))
}

// ---------------------------------------------------------------------------
// AgentSession::new()
// ---------------------------------------------------------------------------

mod session_new_tests {
    use super::*;

    #[test]
    fn new_session_starts_with_running_status() {
        let s = make_session(AgentType::OnDemand);
        assert_eq!(s.status, SessionStatus::Running);
    }

    #[test]
    fn new_session_has_empty_messages() {
        let s = make_session(AgentType::Dispatch);
        assert!(s.messages.is_empty());
    }

    #[test]
    fn new_session_has_empty_actions() {
        let s = make_session(AgentType::Recovery);
        assert!(s.actions.is_empty());
    }

    #[test]
    fn new_session_has_no_outcome() {
        let s = make_session(AgentType::OnDemand);
        assert!(s.outcome.is_none());
    }

    #[test]
    fn new_session_has_no_escalation_reason() {
        let s = make_session(AgentType::OnDemand);
        assert!(s.escalation_reason.is_none());
    }

    #[test]
    fn new_session_has_no_completed_at() {
        let s = make_session(AgentType::OnDemand);
        assert!(s.completed_at.is_none());
    }

    #[test]
    fn new_session_has_no_confidence_score() {
        let s = make_session(AgentType::Anomaly);
        assert!(s.confidence_score.is_none());
    }

    #[test]
    fn new_session_stores_agent_type() {
        let s = make_session(AgentType::MerchantSupport);
        assert_eq!(s.agent_type, AgentType::MerchantSupport);
    }

    #[test]
    fn new_session_stores_trigger() {
        let trigger = json!({"shipment_id": "123"});
        let s = AgentSession::new(TenantId::new(), AgentType::Dispatch, trigger.clone());
        assert_eq!(s.trigger, trigger);
    }
}

// ---------------------------------------------------------------------------
// AgentSession lifecycle transitions
// ---------------------------------------------------------------------------

mod session_lifecycle_tests {
    use super::*;

    #[test]
    fn add_message_appends_to_messages() {
        let mut s = make_session(AgentType::OnDemand);
        s.messages.push(make_message(MessageRole::User));
        assert_eq!(s.messages.len(), 1);
        s.messages.push(make_message(MessageRole::Assistant));
        assert_eq!(s.messages.len(), 2);
    }

    #[test]
    fn record_action_appends_to_actions() {
        let mut s = make_session(AgentType::Dispatch);
        let a = make_action(s.id, "assign_driver");
        s.actions.push(a);
        assert_eq!(s.actions.len(), 1);
    }

    #[test]
    fn complete_sets_status_to_completed() {
        let mut s = make_session(AgentType::OnDemand);
        s.complete("Driver assigned successfully.".into(), 0.95);
        assert_eq!(s.status, SessionStatus::Completed);
    }

    #[test]
    fn complete_sets_outcome() {
        let mut s = make_session(AgentType::OnDemand);
        s.complete("Task done.".into(), 0.9);
        assert_eq!(s.outcome, Some("Task done.".into()));
    }

    #[test]
    fn complete_sets_confidence_score() {
        let mut s = make_session(AgentType::OnDemand);
        s.complete("Done.".into(), 0.87);
        assert!((s.confidence_score.unwrap() - 0.87).abs() < 0.001);
    }

    #[test]
    fn complete_sets_completed_at() {
        let mut s = make_session(AgentType::OnDemand);
        assert!(s.completed_at.is_none());
        s.complete("Done.".into(), 1.0);
        assert!(s.completed_at.is_some());
    }

    #[test]
    fn fail_sets_status_to_failed() {
        let mut s = make_session(AgentType::Recovery);
        s.fail("Claude API timeout".into());
        assert_eq!(s.status, SessionStatus::Failed);
    }

    #[test]
    fn fail_stores_reason() {
        let mut s = make_session(AgentType::Recovery);
        s.fail("External service unavailable".into());
        assert_eq!(
            s.escalation_reason,
            Some("External service unavailable".into())
        );
    }

    #[test]
    fn fail_sets_completed_at() {
        let mut s = make_session(AgentType::Recovery);
        s.fail("Error".into());
        assert!(s.completed_at.is_some());
    }

    #[test]
    fn escalate_sets_status_to_human_escalated() {
        let mut s = make_session(AgentType::Dispatch);
        s.escalate("No drivers available within 10km".into());
        assert_eq!(s.status, SessionStatus::HumanEscalated);
    }

    #[test]
    fn escalate_stores_reason() {
        let mut s = make_session(AgentType::Dispatch);
        let reason = "3 consecutive delivery failures".to_owned();
        s.escalate(reason.clone());
        assert_eq!(s.escalation_reason, Some(reason));
    }

    #[test]
    fn escalate_sets_completed_at() {
        let mut s = make_session(AgentType::Dispatch);
        s.escalate("No drivers".into());
        assert!(s.completed_at.is_some());
    }

    #[test]
    fn escalation_reason_none_on_successful_completion() {
        let mut s = make_session(AgentType::OnDemand);
        s.complete("Completed fine.".into(), 0.99);
        assert!(s.escalation_reason.is_none());
    }
}

// ---------------------------------------------------------------------------
// Turn-cap escalation (mirrors the MAX_TURNS = 20 guard in AgentRunner)
// ---------------------------------------------------------------------------

mod turn_cap_tests {
    use super::*;

    #[test]
    fn session_can_be_escalated_after_max_turns() {
        let mut s = make_session(AgentType::OnDemand);
        // Simulate what AgentRunner does when MAX_TURNS is exceeded.
        let reason = "Agent exceeded 20 turns without completing".to_owned();
        s.escalate(reason.clone());
        assert_eq!(s.status, SessionStatus::HumanEscalated);
        assert_eq!(s.escalation_reason, Some(reason));
    }

    #[test]
    fn session_accumulates_20_actions_without_panic() {
        let mut s = make_session(AgentType::OnDemand);
        for i in 0..20 {
            let a = make_action(s.id, &format!("tool_{}", i));
            s.actions.push(a);
        }
        assert_eq!(s.actions.len(), 20);
    }
}

// ---------------------------------------------------------------------------
// AgentType serialization and display
// ---------------------------------------------------------------------------

mod agent_type_tests {
    use super::*;

    #[test]
    fn dispatch_serializes_to_snake_case() {
        let v = serde_json::to_value(&AgentType::Dispatch).unwrap();
        assert_eq!(v.as_str().unwrap(), "dispatch");
    }

    #[test]
    fn recovery_serializes_to_snake_case() {
        let v = serde_json::to_value(&AgentType::Recovery).unwrap();
        assert_eq!(v.as_str().unwrap(), "recovery");
    }

    #[test]
    fn reconciliation_serializes_to_snake_case() {
        let v = serde_json::to_value(&AgentType::Reconciliation).unwrap();
        assert_eq!(v.as_str().unwrap(), "reconciliation");
    }

    #[test]
    fn anomaly_serializes_to_snake_case() {
        let v = serde_json::to_value(&AgentType::Anomaly).unwrap();
        assert_eq!(v.as_str().unwrap(), "anomaly");
    }

    #[test]
    fn merchant_support_serializes_to_snake_case() {
        let v = serde_json::to_value(&AgentType::MerchantSupport).unwrap();
        assert_eq!(v.as_str().unwrap(), "merchant_support");
    }

    #[test]
    fn on_demand_serializes_to_snake_case() {
        let v = serde_json::to_value(&AgentType::OnDemand).unwrap();
        assert_eq!(v.as_str().unwrap(), "on_demand");
    }

    #[test]
    fn all_six_agent_types_deserialize_correctly() {
        let types = [
            ("dispatch", AgentType::Dispatch),
            ("recovery", AgentType::Recovery),
            ("reconciliation", AgentType::Reconciliation),
            ("anomaly", AgentType::Anomaly),
            ("merchant_support", AgentType::MerchantSupport),
            ("on_demand", AgentType::OnDemand),
        ];
        for (s, expected) in types {
            let got: AgentType = serde_json::from_str(&format!("\"{}\"", s))
                .unwrap_or_else(|e| panic!("Failed to deserialize '{}': {}", s, e));
            assert_eq!(got, expected);
        }
    }

    #[test]
    fn dispatch_display_name() {
        assert_eq!(AgentType::Dispatch.display_name(), "Dispatch Agent");
    }

    #[test]
    fn recovery_display_name() {
        assert_eq!(AgentType::Recovery.display_name(), "Recovery Agent");
    }

    #[test]
    fn reconciliation_display_name() {
        assert_eq!(AgentType::Reconciliation.display_name(), "Reconciliation Agent");
    }

    #[test]
    fn anomaly_display_name() {
        assert_eq!(AgentType::Anomaly.display_name(), "Anomaly Detection Agent");
    }

    #[test]
    fn merchant_support_display_name() {
        assert_eq!(AgentType::MerchantSupport.display_name(), "Merchant Support Agent");
    }

    #[test]
    fn on_demand_display_name() {
        assert_eq!(AgentType::OnDemand.display_name(), "On-Demand Agent");
    }
}

// ---------------------------------------------------------------------------
// SessionStatus PartialEq
// ---------------------------------------------------------------------------

mod session_status_tests {
    use super::*;

    #[test]
    fn running_equals_running() {
        assert_eq!(SessionStatus::Running, SessionStatus::Running);
    }

    #[test]
    fn completed_equals_completed() {
        assert_eq!(SessionStatus::Completed, SessionStatus::Completed);
    }

    #[test]
    fn running_not_equal_to_completed() {
        assert_ne!(SessionStatus::Running, SessionStatus::Completed);
    }

    #[test]
    fn failed_not_equal_to_human_escalated() {
        assert_ne!(SessionStatus::Failed, SessionStatus::HumanEscalated);
    }

    #[test]
    fn running_serializes_to_running() {
        let v = serde_json::to_value(&SessionStatus::Running).unwrap();
        assert_eq!(v.as_str().unwrap(), "running");
    }

    #[test]
    fn completed_serializes_to_completed() {
        let v = serde_json::to_value(&SessionStatus::Completed).unwrap();
        assert_eq!(v.as_str().unwrap(), "completed");
    }

    #[test]
    fn failed_serializes_to_failed() {
        let v = serde_json::to_value(&SessionStatus::Failed).unwrap();
        assert_eq!(v.as_str().unwrap(), "failed");
    }

    #[test]
    fn human_escalated_serializes_to_human_escalated() {
        let v = serde_json::to_value(&SessionStatus::HumanEscalated).unwrap();
        assert_eq!(v.as_str().unwrap(), "human_escalated");
    }
}

// ---------------------------------------------------------------------------
// AgentAction::new()
// ---------------------------------------------------------------------------

mod agent_action_tests {
    use super::*;

    #[test]
    fn new_action_has_unique_id() {
        let sid = Uuid::new_v4();
        let a1 = make_action(sid, "assign_driver");
        let a2 = make_action(sid, "assign_driver");
        assert_ne!(a1.id, a2.id, "Each action should have a unique UUID");
    }

    #[test]
    fn new_action_is_not_succeeded() {
        let a = make_action(Uuid::new_v4(), "get_available_drivers");
        assert!(!a.succeeded, "New actions should not be marked succeeded by default");
    }

    #[test]
    fn new_action_has_no_tool_result() {
        let a = make_action(Uuid::new_v4(), "get_driver_location");
        assert!(a.tool_result.is_none());
    }

    #[test]
    fn new_action_stores_tool_name() {
        let a = make_action(Uuid::new_v4(), "optimize_route");
        assert_eq!(a.tool_name, "optimize_route");
    }

    #[test]
    fn new_action_stores_tool_input() {
        let input = json!({"route_id": "abc123"});
        let a = AgentAction::new(Uuid::new_v4(), "optimize_route".into(), input.clone());
        assert_eq!(a.tool_input, input);
    }

    #[test]
    fn new_action_stores_session_id() {
        let sid = Uuid::new_v4();
        let a = make_action(sid, "send_notification");
        assert_eq!(a.session_id, sid);
    }

    #[test]
    fn action_can_be_marked_succeeded() {
        let mut a = make_action(Uuid::new_v4(), "assign_driver");
        a.succeeded = true;
        assert!(a.succeeded);
    }

    #[test]
    fn action_tool_result_can_be_set() {
        let mut a = make_action(Uuid::new_v4(), "assign_driver");
        a.tool_result = Some(json!({"driver_id": "drv_123", "eta_minutes": 8}));
        assert!(a.tool_result.is_some());
        assert_eq!(a.tool_result.unwrap()["driver_id"], "drv_123");
    }
}
