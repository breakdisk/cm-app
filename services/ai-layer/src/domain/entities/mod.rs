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
            Self::OnDemand       => "On-Demand Agent",
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

// ---------------------------------------------------------------------------
// Tool definition (MCP tool schema for Claude's tool_use feature)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name:        String,
    pub description: String,
    pub input_schema: serde_json::Value,  // JSON Schema object
}
