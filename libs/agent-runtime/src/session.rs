//! Agent session, messages and the append-only action audit log.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use logisticos_types::TenantId;

use crate::role::AgentRole;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Running,
    Completed,
    Failed,
    HumanEscalated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    pub role:    MessageRole,
    /// String, or an array of Claude content blocks.
    pub content: serde_json::Value,
}

/// Immutable audit entry for one tool call. Never updated after `succeeded` is set.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentAction {
    pub id:          Uuid,
    pub session_id:  Uuid,
    pub tool_name:   String,
    pub tool_input:  serde_json::Value,
    pub tool_result: Option<serde_json::Value>,
    pub succeeded:   bool,
    pub executed_at: DateTime<Utc>,
}

impl AgentAction {
    pub fn new(session_id: Uuid, tool_name: String, tool_input: serde_json::Value) -> Self {
        Self {
            id: Uuid::new_v4(),
            session_id,
            tool_name,
            tool_input,
            tool_result: None,
            succeeded: false,
            executed_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSession {
    pub id:                Uuid,
    pub tenant_id:         TenantId,
    pub role:              AgentRole,
    pub status:            SessionStatus,
    pub trigger:           serde_json::Value,
    pub messages:          Vec<AgentMessage>,
    pub actions:           Vec<AgentAction>,
    pub outcome:           Option<String>,
    pub escalation_reason: Option<String>,
    pub confidence_score:  Option<f32>,
    pub model_used:        String,
    /// Set on a specialist's session, pointing at the mesh run that spawned
    /// it. `None` on a root session.
    pub parent_session_id: Option<Uuid>,
    pub started_at:        DateTime<Utc>,
    pub completed_at:      Option<DateTime<Utc>>,
}

impl AgentSession {
    /// `model` is recorded on the session for audit. It is supplied rather than
    /// hardcoded so the recorded model can never drift from the one actually called.
    pub fn new(
        tenant_id: TenantId,
        role: AgentRole,
        trigger: serde_json::Value,
        model: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            role,
            status: SessionStatus::Running,
            trigger,
            messages: Vec::new(),
            actions: Vec::new(),
            outcome: None,
            escalation_reason: None,
            confidence_score: None,
            model_used: model.into(),
            parent_session_id: None,
            started_at: Utc::now(),
            completed_at: None,
        }
    }

    /// A specialist's session inside a mesh run.
    pub fn new_child(
        tenant_id: TenantId,
        role: AgentRole,
        trigger: serde_json::Value,
        model: impl Into<String>,
        parent_session_id: Uuid,
    ) -> Self {
        let mut s = Self::new(tenant_id, role, trigger, model);
        s.parent_session_id = Some(parent_session_id);
        s
    }

    pub fn complete(&mut self, outcome: String, confidence: f32) {
        self.status = SessionStatus::Completed;
        self.outcome = Some(outcome);
        self.confidence_score = Some(confidence);
        self.completed_at = Some(Utc::now());
    }

    pub fn fail(&mut self, reason: String) {
        self.status = SessionStatus::Failed;
        self.escalation_reason = Some(reason);
        self.completed_at = Some(Utc::now());
    }

    pub fn escalate(&mut self, reason: String) {
        self.status = SessionStatus::HumanEscalated;
        self.escalation_reason = Some(reason);
        self.completed_at = Some(Utc::now());
    }

    /// Put a finished session back into `Running` so another user turn can be
    /// appended — one row and one audit trail per conversation, not per message.
    pub fn reopen(&mut self) {
        self.status = SessionStatus::Running;
        self.completed_at = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use logisticos_types::TenantId;
    use uuid::Uuid;

    fn role() -> AgentRole {
        AgentRole::unrestricted("planner", "Planner Agent", "You plan work.")
    }

    fn session() -> AgentSession {
        AgentSession::new(
            TenantId::from_uuid(Uuid::new_v4()),
            role(),
            serde_json::json!({}),
            "claude-opus-4-6",
        )
    }

    #[test]
    fn new_session_starts_running_and_records_its_model() {
        let s = session();
        assert_eq!(s.status, SessionStatus::Running);
        assert_eq!(s.model_used, "claude-opus-4-6");
        assert!(s.completed_at.is_none());
    }

    /// `GET /v1/agents/chat/:id` reports `resolved_by_human` as
    /// `status == Completed && escalation_reason.is_some()`. That only
    /// distinguishes an operator-resolved case from an ordinary finished chat
    /// because `complete()` leaves `escalation_reason` in place.
    #[test]
    fn completing_an_escalation_keeps_the_escalation_reason() {
        let mut escalated = session();
        escalated.escalate("customer asked for a human".into());
        escalated.complete("Resolved by human (op-1): refunded".into(), 1.0);

        assert_eq!(escalated.status, SessionStatus::Completed);
        assert!(escalated.escalation_reason.is_some());

        let mut plain = session();
        plain.complete("Here's your tracking update.".into(), 0.9);
        assert_eq!(plain.status, SessionStatus::Completed);
        assert!(plain.escalation_reason.is_none());
    }

    #[test]
    fn reopen_clears_completion_for_the_next_turn() {
        let mut s = session();
        s.complete("done".into(), 0.9);
        s.reopen();
        assert_eq!(s.status, SessionStatus::Running);
        assert!(s.completed_at.is_none());
    }

    #[test]
    fn a_root_session_has_no_parent() {
        assert!(session().parent_session_id.is_none());
    }

    #[test]
    fn a_child_session_records_its_parent() {
        let parent = session();
        let child = AgentSession::new_child(
            parent.tenant_id,
            AgentRole::unrestricted("worker", "Worker", "You work."),
            serde_json::json!({}),
            "claude-opus-4-6",
            parent.id,
        );
        assert_eq!(child.parent_session_id, Some(parent.id));
        assert_ne!(child.id, parent.id);
    }
}
