/// Core domain types for the Agentic Runtime.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use logisticos_types::TenantId;

// ---------------------------------------------------------------------------
// Agent identity
// ---------------------------------------------------------------------------

/// Well-known autonomous agents in the platform.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentType {
    /// Watches shipment.created → auto-assigns optimal driver.
    Dispatch,
    /// Watches delivery.failed → reschedules, notifies, applies SLA penalties.
    Recovery,
    /// Watches cod.collected → detects missing reconciliation, triggers wallet credit.
    Reconciliation,
    /// Monitors analytics stream → detects anomalies, pages ops team.
    Anomaly,
    /// Answers merchant queries about their logistics data.
    MerchantSupport,
    /// Answers end-customer (recipient) queries from the customer app chat.
    /// Deliberately the narrowest agent on the platform — see `allowed_tools`.
    CustomerSupport,
    /// Free-form agent triggered by a human or API caller.
    OnDemand,
}

impl AgentType {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Dispatch       => "Dispatch Agent",
            Self::Recovery       => "Recovery Agent",
            Self::Reconciliation => "Reconciliation Agent",
            Self::Anomaly        => "Anomaly Detection Agent",
            Self::MerchantSupport=> "Merchant Support Agent",
            Self::CustomerSupport=> "Customer Support Agent",
            Self::OnDemand       => "On-Demand Agent",
        }
    }

    /// Tools this agent type is authorised to call.
    ///
    /// `None` means the full registry — the historical behaviour, kept for the
    /// internal/autonomous agents that run on trusted Kafka triggers.
    /// `Some(list)` is an allowlist enforced in the agent loop *and* used to
    /// filter the tool definitions sent to Claude, so a restricted agent is
    /// never even told the other tools exist.
    ///
    /// This closes ADR-0004's "the Support Agent must not be able to call
    /// `assign_driver`" requirement for the customer-facing surface: an end
    /// customer chatting in the mobile app must never reach dispatch, billing,
    /// driver-instruction or analytics tools.
    pub fn allowed_tools(&self) -> Option<&'static [&'static str]> {
        match self {
            Self::CustomerSupport => Some(&[
                "get_shipment",
                "reschedule_delivery",
                "escalate_to_human",
            ]),
            _ => None,
        }
    }

    /// System prompt snippet describing this agent's role and constraints.
    pub fn system_context(&self) -> &'static str {
        match self {
            Self::Dispatch => {
                "You are the LogisticOS Dispatch Agent. Your job is to assign the optimal available \
                 driver to shipments. You must: 1) Find available drivers near the pickup location, \
                 2) Score them by distance and current workload, 3) Assign the best-scoring driver. \
                 Only escalate to a human if no drivers are available within 10km."
            }
            Self::Recovery => {
                "You are the LogisticOS Recovery Agent. A delivery has failed. Your job is to: \
                 1) Understand the failure reason, 2) Re-schedule the delivery for the next available \
                 slot, 3) Send a customer notification with the new ETA, 4) Apply SLA penalty to the \
                 carrier if applicable. Escalate only if the shipment has failed 3+ times."
            }
            Self::Reconciliation => {
                "You are the LogisticOS Reconciliation Agent. You detect COD collections that have \
                 not been credited to the merchant wallet within 24 hours and trigger the wallet credit. \
                 Never credit an amount without first verifying the COD collection event exists."
            }
            Self::Anomaly => {
                "You are the LogisticOS Anomaly Detection Agent. You monitor delivery metrics and \
                 alert the operations team when: delivery success rate drops below 80%, a driver \
                 has 3+ consecutive failures, or COD collection rate drops below 90%."
            }
            Self::MerchantSupport => {
                "You are the LogisticOS Merchant Support Agent. You have access to a merchant's \
                 shipment data, delivery metrics, and billing records. Answer questions accurately \
                 and concisely. Never reveal data from other tenants."
            }
            Self::CustomerSupport => {
                "You are the LogisticOS customer support assistant, talking directly to an end \
                 customer inside the mobile app. Be warm, brief and concrete — this is a chat \
                 bubble on a phone, so keep replies under about 60 words and never use markdown \
                 formatting, headings or bullet symbols. \
                 \
                 You can look up a shipment and reschedule a delivery. You cannot assign drivers, \
                 issue refunds, change prices, or view other customers' data — if asked, say so \
                 plainly and offer to hand over to a human. \
                 \
                 Only ever discuss the shipments listed in the customer context you were given. \
                 If the customer asks about a tracking number that is not in that list, tell them \
                 you cannot see it on their account and ask them to check the number. \
                 \
                 Before rescheduling a delivery, state the shipment and the new date and get an \
                 explicit yes from the customer. Never reschedule on a first mention. \
                 \
                 Call escalate_to_human when the customer asks for a person, reports a lost, \
                 stolen or damaged parcel, disputes a charge, or when you have failed to resolve \
                 the same issue twice."
            }
            Self::OnDemand => {
                "You are a LogisticOS AI agent with access to logistics operations tools. \
                 Execute the requested task carefully and confirm each step before proceeding \
                 to irreversible operations."
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Agent session — tracks a single agent run from trigger to completion
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Running,
    Completed,
    Failed,
    HumanEscalated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSession {
    pub id:               Uuid,
    pub tenant_id:        TenantId,
    pub agent_type:       AgentType,
    pub status:           SessionStatus,

    /// The event or request that triggered this session.
    pub trigger:          serde_json::Value,

    /// Full message history for this agent run (user + assistant + tool messages).
    pub messages:         Vec<AgentMessage>,

    /// Actions taken during this session (tool calls and their results).
    pub actions:          Vec<AgentAction>,

    /// Final outcome summary written by the agent.
    pub outcome:          Option<String>,

    /// Human escalation reason (if status == HumanEscalated).
    pub escalation_reason: Option<String>,

    pub confidence_score: Option<f32>,  // 0.0 – 1.0, agent's self-reported confidence
    pub model_used:       String,

    pub started_at:       DateTime<Utc>,
    pub completed_at:     Option<DateTime<Utc>>,
}

impl AgentSession {
    pub fn new(tenant_id: TenantId, agent_type: AgentType, trigger: serde_json::Value) -> Self {
        Self {
            id:               Uuid::new_v4(),
            tenant_id,
            agent_type,
            status:           SessionStatus::Running,
            trigger,
            messages:         Vec::new(),
            actions:          Vec::new(),
            outcome:          None,
            escalation_reason: None,
            confidence_score: None,
            model_used:       "claude-opus-4-6".into(),
            started_at:       Utc::now(),
            completed_at:     None,
        }
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
    /// appended. Multi-turn chat is the only caller: every turn re-enters the
    /// agent loop on the same session, keeping one row and one audit trail per
    /// conversation instead of one per message.
    pub fn reopen(&mut self) {
        self.status = SessionStatus::Running;
        self.completed_at = None;
    }
}

// ---------------------------------------------------------------------------
// Messages in the agent conversation (Claude API format)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    pub role:    MessageRole,
    pub content: serde_json::Value,  // string or array of content blocks
}

// ---------------------------------------------------------------------------
// Agent action — immutable audit log entry for each tool call
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentAction {
    pub id:            Uuid,
    pub session_id:    Uuid,
    pub tool_name:     String,
    pub tool_input:    serde_json::Value,
    pub tool_result:   Option<serde_json::Value>,
    pub succeeded:     bool,
    pub executed_at:   DateTime<Utc>,
}

impl AgentAction {
    pub fn new(session_id: Uuid, tool_name: String, tool_input: serde_json::Value) -> Self {
        Self {
            id:          Uuid::new_v4(),
            session_id,
            tool_name,
            tool_input,
            tool_result: None,
            succeeded:   false,
            executed_at: Utc::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The customer-facing agent talks to end customers, so its reachable tool
    /// set is a security boundary, not a convenience. Any tool added to the
    /// registry must stay unreachable here unless deliberately allowlisted.
    #[test]
    fn customer_support_cannot_reach_operational_tools() {
        let allowed = AgentType::CustomerSupport
            .allowed_tools()
            .expect("CustomerSupport must be restricted");

        for forbidden in [
            "assign_driver",
            "generate_invoice",
            "reconcile_cod",
            "send_driver_instruction",
            "get_cod_balance",
            "get_delivery_metrics",
            "get_driver_location",
            "get_churn_score",
            "schedule_dock",
            "get_available_drivers",
            "send_notification",
        ] {
            assert!(
                !allowed.contains(&forbidden),
                "{forbidden} must not be reachable from the customer chat"
            );
        }
    }

    #[test]
    fn customer_support_can_reach_its_three_tools() {
        let allowed = AgentType::CustomerSupport.allowed_tools().unwrap();
        assert_eq!(allowed.len(), 3);
        for expected in ["get_shipment", "reschedule_delivery", "escalate_to_human"] {
            assert!(allowed.contains(&expected), "{expected} should be allowed");
        }
    }

    /// Internal/autonomous agents keep the full registry — restricting them was
    /// not part of this change and would silently break dispatch.
    #[test]
    fn operational_agents_are_unrestricted() {
        for agent in [
            AgentType::Dispatch,
            AgentType::Recovery,
            AgentType::Reconciliation,
            AgentType::Anomaly,
            AgentType::MerchantSupport,
            AgentType::OnDemand,
        ] {
            assert!(agent.allowed_tools().is_none(), "{agent:?} should be unrestricted");
        }
    }

    /// `GET /v1/agents/chat/:id` reports `resolved_by_human` as
    /// `status == Completed && escalation_reason.is_some()`. That only
    /// distinguishes an operator-resolved case from an ordinary finished chat
    /// because `complete()` leaves `escalation_reason` in place.
    #[test]
    fn completing_an_escalation_keeps_the_escalation_reason() {
        let mut escalated = AgentSession::new(
            logisticos_types::TenantId::from_uuid(Uuid::new_v4()),
            AgentType::CustomerSupport,
            serde_json::json!({}),
        );
        escalated.escalate("customer asked for a human".into());
        escalated.complete("Resolved by human (op-1): refunded".into(), 1.0);

        assert_eq!(escalated.status, SessionStatus::Completed);
        assert!(escalated.escalation_reason.is_some(), "human-resolved case must stay distinguishable");

        // An ordinary chat that was never escalated must not look resolved-by-human.
        let mut plain = AgentSession::new(
            logisticos_types::TenantId::from_uuid(Uuid::new_v4()),
            AgentType::CustomerSupport,
            serde_json::json!({}),
        );
        plain.complete("Here's your tracking update.".into(), 0.9);

        assert_eq!(plain.status, SessionStatus::Completed);
        assert!(plain.escalation_reason.is_none());
    }

    #[test]
    fn reopen_puts_a_completed_session_back_in_running() {
        let mut session = AgentSession::new(
            logisticos_types::TenantId::from_uuid(Uuid::new_v4()),
            AgentType::CustomerSupport,
            serde_json::json!({}),
        );
        session.complete("done".into(), 0.9);
        assert_eq!(session.status, SessionStatus::Completed);
        assert!(session.completed_at.is_some());

        session.reopen();

        assert_eq!(session.status, SessionStatus::Running);
        assert!(session.completed_at.is_none());
        // The prior outcome survives — the next turn appends to this history.
        assert_eq!(session.outcome.as_deref(), Some("done"));
    }
}

// ---------------------------------------------------------------------------
// Tool definition (MCP tool schema for Claude's tool_use feature)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name:        String,
    pub description: String,
    pub input_schema: serde_json::Value,  // JSON Schema object
}
