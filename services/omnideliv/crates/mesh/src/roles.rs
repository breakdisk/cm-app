//! Mesh roles.
//!
//! Agents are roles, not singletons: the runner instantiates one worker per
//! sub-intent, so a single Nutritionist role yields two live workers when an
//! utterance splits into restaurant and grocery.
//!
//! Every role carries an explicit allowlist. A restricted role is never told
//! the other tools exist — the filter applies to the definitions sent to
//! Claude, not merely to execution.

use logisticos_agent_runtime::AgentRole;

pub const CONCIERGE_KEY:    &str = "concierge";
pub const NUTRITIONIST_KEY: &str = "nutritionist";
pub const FLEET_KEY:        &str = "fleet";

/// The orchestrator. Reads the customer profile, splits the utterance, and
/// owns the basket — but does no catalog or fleet work itself.
pub fn concierge() -> AgentRole {
    AgentRole::restricted(
        CONCIERGE_KEY,
        "Concierge",
        "You are the OmniDeliv Concierge. A customer has told you what they need in \
         one message. Your only job in this turn is to split it into separate \
         sub-intents, one per vertical (restaurant, grocery, pharmacy, florist, \
         retail). Call decompose_intent exactly once with the full list. \
         \
         Split by vertical, not by item: 'dinner from Kuya's and we're out of milk \
         and eggs' is two sub-intents (restaurant, grocery), not three. Carry any \
         constraint the customer stated — budget, dietary, timing — into the \
         constraints of the sub-intent it applies to. \
         \
         Do not search for products, pick vendors, or estimate delivery. \
         Specialists do that. Never invent a vertical the customer did not ask for.",
        ["get_customer_profile", "decompose_intent", "present_bundle"],
    )
}

/// Food and grocery. Owns dietary filtering, availability reasoning and
/// substitution. Instantiated once per food-or-grocery sub-intent.
pub fn nutritionist() -> AgentRole {
    AgentRole::restricted(
        NUTRITIONIST_KEY,
        "Nutritionist",
        "You are the OmniDeliv Nutritionist, working one sub-intent of a larger \
         order. Call find_vendors first to turn your vertical into real vendors near \
         the customer: your sub-intent's vendor_hint is a name the customer used, not \
         an id, and every catalog tool needs an id. Then find items that satisfy the \
         sub-intent and call propose_lines exactly once with what you chose. \
         \
         Respect every allergen in your constraints absolutely — never propose an \
         item that carries one, and never substitute around a dietary restriction. \
         \
         search_catalog returns a warrants_substitute flag per item. When it is \
         true the item is out of stock, nearly out, or last confirmed present too \
         long ago to rely on — propose a replacement alongside it via \
         propose_substitution so the customer has a choice rather than a failed \
         pickup. Do not silently swap: the customer approves substitutions. \
         \
         If nothing satisfies the sub-intent, call propose_lines with an empty \
         list and a note saying why. An honest empty result is correct; a \
         plausible wrong item is not.",
        ["find_vendors", "search_catalog", "check_availability", "propose_substitution", "propose_lines"],
    )
}

/// Courier supply and routing. Sees no catalog and no customer data — it works
/// from vendor locations and the merged basket alone.
pub fn fleet() -> AgentRole {
    AgentRole::restricted(
        FLEET_KEY,
        "Fleet",
        "You are the OmniDeliv Fleet agent. You have a merged basket spanning one \
         or more vendors. Sequence the pickups and compute one flat delivery fee. \
         \
         Sequence by readiness, not distance alone: a grocery pick ready in 5 \
         minutes should be collected before a kitchen order ready in 20, so hot \
         food spends the least time in the bag. Where a basket mixes hot and \
         chilled items, say so in your plan. \
         \
         The fee is flat regardless of stop count — that is the product promise. \
         Call plan_route exactly once.",
        ["get_available_couriers", "estimate_route", "compute_flat_fee", "plan_route"],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// No agent in any role may hold a tool that moves money or dispatches a
    /// real courier. Those fire from the commit path on an explicit user tap.
    #[test]
    fn no_role_can_reach_a_money_or_dispatch_tool() {
        let forbidden = [
            "charge_customer", "capture_payment", "issue_refund",
            "assign_courier", "dispatch_courier", "generate_invoice",
            "credit_vendor", "debit_courier_ledger",
        ];

        for role in [concierge(), nutritionist(), fleet()] {
            for tool in forbidden {
                assert!(
                    !role.permits(tool),
                    "{} must not reach {tool}", role.key()
                );
            }
        }
    }

    #[test]
    fn the_concierge_cannot_touch_the_catalog_or_the_fleet() {
        let c = concierge();
        assert!(c.permits("get_customer_profile"));
        assert!(c.permits("decompose_intent"));
        assert!(!c.permits("search_catalog"), "the Concierge delegates catalog work");
        assert!(!c.permits("estimate_route"));
    }

    /// A specialist is handed `vendor_hint` — a name the customer said — while
    /// every catalog tool needs a `vendor_id`. Without `find_vendors` the
    /// Nutritionist cannot search anything, and the empty result it would
    /// return is indistinguishable from honest "nothing satisfies this".
    #[test]
    fn the_nutritionist_can_resolve_a_vendor_before_searching() {
        assert!(
            nutritionist().permits("find_vendors"),
            "a specialist with catalog tools but no way to find a vendor is inert",
        );
    }

    #[test]
    fn the_nutritionist_reaches_the_catalog_but_not_the_fleet() {
        let n = nutritionist();
        assert!(n.permits("search_catalog"));
        assert!(n.permits("check_availability"));
        assert!(n.permits("propose_substitution"));
        assert!(!n.permits("estimate_route"));
        assert!(!n.permits("get_customer_profile"), "specialists get constraints passed in, not PII access");
    }

    #[test]
    fn the_fleet_agent_sees_no_catalog_and_no_customer_data() {
        let f = fleet();
        assert!(f.permits("get_available_couriers"));
        assert!(f.permits("estimate_route"));
        assert!(f.permits("compute_flat_fee"));
        assert!(!f.permits("search_catalog"));
        assert!(!f.permits("get_customer_profile"));
    }

    /// Every role is restricted. An unrestricted role here would silently grant
    /// the full registry — the failure mode this gate exists to prevent.
    #[test]
    fn every_mesh_role_is_restricted() {
        for role in [concierge(), nutritionist(), fleet()] {
            assert!(
                role.allowed_tools().is_some(),
                "{} must carry an explicit allowlist", role.key()
            );
        }
    }

    /// The roles partition the tools: no tool is reachable by two roles. If a
    /// capability ever genuinely needs sharing, this failing is the prompt to
    /// say so out loud rather than let authority drift outward quietly.
    #[test]
    fn the_three_roles_share_no_tool() {
        let roles = [concierge(), nutritionist(), fleet()];
        for (i, a) in roles.iter().enumerate() {
            for b in roles.iter().skip(i + 1) {
                for tool in a.allowed_tools().expect("restricted") {
                    assert!(
                        !b.permits(tool),
                        "{} and {} both reach {tool}", a.key(), b.key()
                    );
                }
            }
        }
    }
}
