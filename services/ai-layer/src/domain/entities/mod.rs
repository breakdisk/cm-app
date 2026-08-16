/// Logistics agent identity.
///
/// The session entities, the agent loop and the RBAC gate now live in
/// `libs/agent-runtime`, shared with other products. `AgentType` stays here
/// because it is irreducibly a LogisticOS concept; it converts into the
/// runtime's product-agnostic `AgentRole` at session construction.
use serde::{Deserialize, Serialize};

pub use logisticos_agent_runtime::{
    AgentAction, AgentMessage, AgentRole, AgentSession, MessageRole, SessionStatus, ToolDefinition,
};

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

impl AgentType {
    /// The stable string this variant persists as — the value stored in the
    /// `agent_type` column and carried as the `AgentRole` key.
    ///
    /// Derived from the serde representation rather than hand-written so the
    /// two can never drift apart.
    pub fn key(&self) -> String {
        serde_json::to_value(self)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .expect("AgentType must serialise to a string")
    }

    /// True when `role` is this variant's runtime form.
    ///
    /// `AgentSession` now holds an opaque `AgentRole` instead of this enum, so
    /// the "is this a customer-support session?" guards compare through here.
    pub fn matches_role(&self, role: &AgentRole) -> bool {
        role.key() == self.key()
    }
}

impl From<AgentType> for AgentRole {
    fn from(t: AgentType) -> Self {
        let key = t.key();

        match t.allowed_tools() {
            None => AgentRole::unrestricted(key, t.display_name(), t.system_context()),
            Some(allowed) => {
                AgentRole::restricted(key, t.display_name(), t.system_context(), allowed.iter().copied())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;
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
            AgentType::CustomerSupport.into(),
            serde_json::json!({}),
            "test-model",
        );
        escalated.escalate("customer asked for a human".into());
        escalated.complete("Resolved by human (op-1): refunded".into(), 1.0);

        assert_eq!(escalated.status, SessionStatus::Completed);
        assert!(escalated.escalation_reason.is_some(), "human-resolved case must stay distinguishable");

        // An ordinary chat that was never escalated must not look resolved-by-human.
        let mut plain = AgentSession::new(
            logisticos_types::TenantId::from_uuid(Uuid::new_v4()),
            AgentType::CustomerSupport.into(),
            serde_json::json!({}),
            "test-model",
        );
        plain.complete("Here's your tracking update.".into(), 0.9);

        assert_eq!(plain.status, SessionStatus::Completed);
        assert!(plain.escalation_reason.is_none());
    }

    #[test]
    fn reopen_puts_a_completed_session_back_in_running() {
        let mut session = AgentSession::new(
            logisticos_types::TenantId::from_uuid(Uuid::new_v4()),
            AgentType::CustomerSupport.into(),
            serde_json::json!({}),
            "test-model",
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

    /// `AgentRole.key` is the persisted `agent_type` column value. If this
    /// drifts from the enum's serde representation, existing rows stop loading.
    #[test]
    fn agent_role_key_matches_the_persisted_enum_string() {
        for agent in [
            AgentType::Dispatch,
            AgentType::Recovery,
            AgentType::Reconciliation,
            AgentType::Anomaly,
            AgentType::MerchantSupport,
            AgentType::CustomerSupport,
            AgentType::OnDemand,
        ] {
            let serialised = serde_json::to_value(&agent).unwrap();
            let expected = serialised.as_str().expect("AgentType serialises to a string");
            let role: AgentRole = agent.clone().into();
            assert_eq!(role.key(), expected, "{agent:?} key must match its serde string");
        }
    }

    /// The customer-facing agent must stay the narrowest role on the platform
    /// after conversion — the same guarantee the pre-extraction test made.
    #[test]
    fn customer_support_role_stays_restricted_after_conversion() {
        let role: AgentRole = AgentType::CustomerSupport.into();
        for forbidden in [
            "assign_driver", "generate_invoice", "reconcile_cod", "send_driver_instruction",
            "get_cod_balance", "get_delivery_metrics", "get_driver_location", "get_churn_score",
            "schedule_dock", "get_available_drivers", "send_notification",
        ] {
            assert!(!role.permits(forbidden), "{forbidden} must not be reachable from customer chat");
        }
        for allowed in ["get_shipment", "reschedule_delivery", "escalate_to_human"] {
            assert!(role.permits(allowed), "{allowed} should be allowed");
        }
    }
}
