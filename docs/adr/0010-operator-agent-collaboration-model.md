# ADR-0010: Operator-Agent Collaboration Model

**Status:** Accepted
**Date:** 2026-04-12
**Deciders:** Principal Architect, Staff ML Engineer, CPO, CTO

## Context

CargoMarket positions itself as **Agentic SaaS**, not "SaaS with a chatbot." The AI Intelligence Layer (Service 16, see ADR-0004) is the runtime that operates the platform — Dispatch Agent, Support Agent, Billing Agent, Marketing Agent, Operations Copilot all reach into operational services exclusively via MCP.

This raises a question that every Agentic SaaS team must answer before building the AI Layer in earnest: **what is the relationship between the AI agent and the human operator?**

The naive mental models all fail:

1. **"Agent does easy stuff, operator does hard stuff."** The line between easy and hard moves every week as the agent improves. There is no stable handoff protocol.
2. **"Operator approves every agent action."** Throughput collapses to human speed. The agent becomes a slow autocomplete. Sales pitch fails.
3. **"Agent acts, operator audits afterward."** No way to roll back side-effects (drivers dispatched, customers messaged, money moved). Audit becomes blame, not control.
4. **"Agent has its own database and its own API."** Two sources of truth, two RBAC systems, two audit trails. The agent becomes a parallel system that drifts from the rest.

None of these scale. We need a model that:

- Lets the agent act fast on routine work (otherwise the value prop dies)
- Keeps a human in the loop where it matters (otherwise trust dies)
- Has a clean audit trail for both actors (otherwise compliance dies)
- Lets operators *teach* the agent through their daily work (otherwise the agent never improves)
- Survives the agent making mistakes without taking the system down

The decision in this ADR is the contract between operator and agent — not a policy document, but a system design that is enforced in code at the API, RBAC, audit, and escalation layers.

## Decision

Adopt a **five-layer collaboration model** between operator and AI agent. Every action in the system lives at exactly one layer, defined by risk, reversibility, and the agent's competence envelope. The layer for any action type can shift over time as confidence grows or shrinks.

The five layers:

| # | Layer | Who acts | Who is notified | Example |
|---|-------|----------|-----------------|---------|
| 1 | **Agent autonomous** | Agent | No one | Routine driver assignment, status notifications, per-shipment invoice on POD |
| 2 | **Agent acts, operator notified** | Agent | Operator (post-fact, can reverse within window) | Mid-shift driver reassignment, small refund under threshold, auto-reschedule on missed delivery |
| 3 | **Agent proposes, operator decides** | Operator | Agent provides recommendation + reasoning | High-value shipment reassignment, large refund, sub-carrier offer acceptance, cancellation |
| 4 | **Operator acts, agent assists** | Operator | Agent serves as copilot (data, drafts, suggestions) | Angry merchant call, fraud investigation, new merchant onboarding |
| 5 | **Operator only, agent excluded** | Operator | Agent forbidden by policy | Legal/contract signing, hiring/firing, crisis comms, GDPR erasure execution |

The layer for an action is **not a runtime confidence threshold**. It is a **policy decision** encoded in the **competence envelope**.

## The Competence Envelope

Each agent (Dispatch Agent, Support Agent, Billing Agent, Marketing Agent) has a **competence envelope** — a precise, versioned, per-Partner contract describing what it can do autonomously vs what it must escalate.

The envelope is a multi-dimensional rule set, not a single confidence threshold:

```
Dispatch Agent envelope (v2.4, Partner X):
  CAN autonomously:
    - Assign drivers to shipments worth ≤ ₱5,000
    - Re-route during normal traffic conditions
    - Reassign tasks when driver goes offline
    - Send routine status notifications
  MUST escalate to operator (Layer 3):
    - Shipments worth > ₱5,000
    - Cross-border legs
    - Repeated reassignment of the same shipment (3+ times)
    - Any action where confidence < 0.7
    - Any action that would breach an SLA
    - First-time customer (no history)
  MUST NEVER (Layer 5):
    - Cancel a shipment without operator approval
    - Assign a shipment to a driver flagged for review
    - Take any action during a P0 incident (kill switch active)
```

### Envelope properties

1. **Per-Partner.** A small Partner with 5 drivers wants the agent conservative. A national Partner with 200 drivers wants it aggressive. Each Partner configures their own envelope in the Partner Portal.
2. **Versioned.** Widening the envelope is a deliberate, recorded decision: "from 2026-04-15, dispatch agent can autonomously handle shipments up to ₱10,000, expanded after 3 months at 99.8% accuracy at the previous threshold." Like any deploy, you can roll back.
3. **Auto-shrinking on bad outcome.** A bad outcome (customer complaint, SLA breach traced to agent action, dispute) automatically reverts the agent to a more conservative envelope until human investigation. Circuit-breaker pattern.
4. **Editable by Partner admin, not CargoMarket support.** The Partner is the principal who decides how much autonomy the agent has on their behalf. The platform provides safe defaults; the Partner can tighten or loosen.
5. **Audited.** Every envelope change is logged with `actor_id`, `from_version`, `to_version`, `reason`.

## Architectural Enforcement

The collaboration model is **not policy** — it is enforced in code at four points. Without these enforcement points, the model degrades into wishful thinking.

### 1. Single API surface for all actors

There is **no agent API** and **no operator API**. There is one operational API per service. When an action arrives:

- The authorization layer extracts `actor_type` (`operator | ai_agent | system | customer | merchant`) and `actor_id` from the request context.
- RBAC checks the actor's authority for this action type, scoped by tenant + role + (for agents) competence envelope.
- If the actor is `ai_agent` and the action is outside its envelope, the request is **denied with an escalation directive**, not silently allowed.

This is critical: if the agent is calling the same API the operator calls, you cannot have agent-only backdoors. The agent's authority is *less* than a senior operator and *more* than a junior operator — and it's encoded in the same RBAC system as humans.

### 2. Unified audit log

```sql
CREATE TABLE audit_log (
  id                       UUID PRIMARY KEY,
  tenant_id                UUID NOT NULL,
  action_type              TEXT NOT NULL,
  target_id                UUID,
  target_type              TEXT,

  actor_type               TEXT NOT NULL CHECK (actor_type IN
                             ('operator', 'ai_agent', 'system', 'customer', 'merchant', 'driver')),
  actor_id                 UUID,

  -- Agent-specific fields (NULL when actor is human)
  agent_name               TEXT,             -- 'dispatch_agent', 'support_agent'
  agent_version            TEXT,             -- 'v2.4'
  agent_confidence         NUMERIC(4,3),     -- 0.000 to 1.000
  agent_reasoning_trace_id UUID,             -- pointer to LangGraph trace + MCP call sequence

  -- Supervision link
  supervising_operator_id  UUID,             -- set if operator approved/acknowledged
  supervision_layer        SMALLINT,         -- 1..5

  -- Reversibility
  reversible               BOOLEAN NOT NULL DEFAULT false,
  reversible_until         TIMESTAMPTZ,
  reversed_by              UUID,
  reversed_at              TIMESTAMPTZ,

  outcome                  TEXT NOT NULL,    -- 'success', 'failure', 'reversed'
  created_at               TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

When something goes wrong six months later, you can answer in one query: **who did this, why did they think it was right, and was a human in the loop?**

The audit log lives in the platform tier (cross-product), since the same operators and agents act across LogisticOS, Carwash, Maintenance, MICE, Ride-Hailing, Food Delivery (per ADR-0009).

### 3. Escalation queue as a first-class object

When the agent decides to escalate (Layer 3), it does not just ping someone — it creates a **structured `EscalationCase`** with the full decision context:

```
EscalationCase {
  id                  UUID
  tenant_id           UUID
  agent_name          TEXT
  agent_version       TEXT
  case_type           TEXT       -- 'high_value_dispatch', 'refund_request', 'cancel_shipment'

  context_bundle      JSONB      -- everything the agent loaded into its context window
  recommendation      JSONB      -- structured proposed action(s)
  agent_confidence    NUMERIC
  agent_reasoning     TEXT       -- natural-language explanation
  reasoning_trace_id  UUID       -- LangGraph trace, MCP tool call sequence

  reversibility_window INTERVAL  -- how long until effects lock in
  suggested_talk_track TEXT      -- if customer-facing

  priority            SMALLINT   -- 1 (urgent) .. 5 (whenever)
  business_value      NUMERIC    -- estimated $ at stake

  status              TEXT       -- 'pending', 'in_review', 'resolved', 'expired'
  assigned_operator   UUID
  resolution          TEXT       -- 'accepted', 'modified', 'rejected', 'overridden'
  resolution_action   JSONB      -- what actually happened
  resolved_at         TIMESTAMPTZ
}
```

The operator's UI is **not a dispatch console with the agent bolted on.** It is an **escalation case queue**, prioritized by urgency × business value. The agent has already done the dispatch console work for the routine cases. The operator's screen is the exception screen.

### 4. Override-as-training-signal pipeline

Every time an operator overrides the agent — accepts with modification, rejects, takes a different action — that becomes feedback for the next agent version:

| Operator action | Training signal |
|-----------------|-----------------|
| **Reject with reason** | Constraint added to the agent's planning prompt or eval set. "Do not recommend X when Y." |
| **Modify the recommendation** | Preference signal: "agent suggested X, operator did Y." Goes into preference dataset. |
| **Accept after long deliberation** | Confidence signal: agent's confidence on this case type was probably too high; should have escalated less or surfaced more context. |
| **Accept quickly** | Positive signal: agent's recommendation was good. Used to consider widening the envelope on this case type. |

This is **not** RLHF in the live system. It is offline training data + eval cases that inform the **next agent version**. The operator's daily work makes the next deploy better. **This is the loop that earns the name "Agentic SaaS."** Without it, the agent is just a static automation script.

The training pipeline lives in MLOps (managed by the AI Layer team). Operator overrides flow via Kafka (`logisticos.agent.override`) into a labeled dataset partitioned per agent + per Partner. Partners can opt out of contributing to cross-Partner training; their data still trains a Partner-private model variant.

## Agent Architecture Implications

### The agent is multi-tenant aware

Each Partner gets their own agent instance(s) — same code, same model, different competence envelope, different prompt context, different memory. RLS at the MCP layer enforces that Partner X's agent never touches Partner Y's data, even if a misbehaving prompt tries.

### The agent has no private database

The agent reads and writes through the same operational services the operator's portal uses. No separate "agent database." Single source of truth. The agent's "memory" is structured into:

- **Per-conversation context** (short-term, in the LLM context window)
- **Per-customer episodic memory** (in the CDP, queryable by other agents and by humans)
- **Per-tenant policy memory** (the competence envelope, prompt overrides, learned constraints)
- **Cross-tenant model weights** (the foundation model itself + any fine-tunes)

### The agent runtime is metered per Partner

Token spend, MCP call count, escalation rate are all metered and attributed to the Partner. They show up in the Partner's CargoMarket invoice. This makes the agent a **billable feature**, not a hidden cost. It also creates a healthy pressure: Partners who run the agent aggressively pay more, Partners who keep it conservative pay less.

### Versioned and rollback-able

The agent is treated like any other service:
- Versioned (`dispatch_agent v2.3 → v2.4`)
- Deployed via the same CI/CD as Rust services
- Rollback-able: "Dispatch Agent v2.4 misbehaved on cross-border shipments — rolled back to v2.3 at 14:32 UTC, incident logged."
- Has a kill switch: `agent_disabled` flag per Partner per agent. When set, all actions for that agent revert to Layer 5 (operator only).

### Cross-product agent deployment

Per ADR-0009, the AI Layer is a **platform-tier** service. The same Dispatch Agent code can run across LogisticOS, Ride-Hailing, Food Delivery — each with its own competence envelope and MCP toolset. Support Agent and Marketing Agent run across all six products. This is why the audit log, escalation queue, and training pipeline live at the platform tier, not inside any one product.

## Operator Job Description (the second-order effect)

The operator's job description **changes** under this model. This is not a side note — it is a deliberate consequence the system is designed to produce.

**Old job:** dispatch shipments, monitor drivers, handle exceptions, answer customer support escalations, manually run reports.

**New job:**
- **Supervise an AI workforce.** The agent is the dispatcher. The operator is the dispatcher's manager.
- **Resolve escalations.** The agent has already done the analysis. Operator decides.
- **Teach the agent.** Every override, every comment, every rejection is signal for the next version. Operators are the SMEs who shape the agent's judgment.
- **Handle the human moments.** Anything that requires empathy, relationship, or accountability — operator-only.
- **Watch for drift.** When the agent starts behaving oddly (confidence dropping, escalations rising, unusual patterns), operator raises the alarm and the platform investigates.
- **Tune the envelope.** Operator (with Partner admin approval) widens or narrows the competence envelope as their confidence in the agent grows.

The leverage equation: **a Partner with 200 shipments per day used to need 5 operators. With the agent, they need 1.** That 1 operator is more skilled, more strategic, more valuable than the 5 they replaced. They are also paid more and quit less.

This is the actual sales pitch for Agentic SaaS to Partners: not "AI replaces people" (fragile and politically toxic in the logistics market), but **"AI raises every operator's leverage 5x."** The Partner keeps their best people and lets the agent absorb the routine work that was burning them out.

## Worked Example — End-to-End Shipment with Mixed Layers

A single LogisticOS shipment, with the actor at every step marked. This is the system in motion.

### Happy path

1. **Customer** opens the customer app, books a Makati → Quezon City pickup. Pays via card. *(Layer 1: customer self-service)*
2. **System** creates `Shipment` in order-intake.
3. **Dispatch Agent** queries `get_available_drivers`, picks Driver Alice, calls `assign_driver`. Confidence 0.96. *(Layer 1: agent autonomous, no operator notification)*
4. **Driver Alice** drives to pickup, captures POD on pickup.
5. **Dispatch Agent** sends "Your package is on the way" via push + WhatsApp. *(Layer 1)*
6. **Driver Alice** drives to destination. Recipient not home. Driver marks "delivery attempted."
7. **Dispatch Agent** evaluates: low-value (₱400), customer has app, auto-reschedule policy in place. Reschedules for tomorrow morning, notifies customer via push, **notifies operator post-fact**: "auto-rescheduled LSPH123, customer notified." *(Layer 2)*
8. **Operator** glances at notification feed, no action needed.
9. **Tomorrow morning, Dispatch Agent** reassigns to Driver Bob. *(Layer 1)*
10. **Driver Bob** delivers, captures POD.
11. **POD service** publishes `pod.captured`.
12. **Billing Agent** consumes `pod.captured`, generates per-shipment invoice via payments service, publishes `InvoiceGenerated`. *(Layer 1)*
13. **Engagement Agent** consumes `InvoiceGenerated`, fans out to in-app + WhatsApp + email per channel preferences in CDP. *(Layer 1)*
14. **Customer** rates the delivery 5 stars.
15. **Marketing Agent** sees 5-star rating, adds customer to "happy customers" segment for referral campaign. *(Layer 1)*

**Operator actions in happy path: zero.**

### Exception path (added at step 7)

7a. **Customer** sends angry WhatsApp: "you missed the delivery, refund me now."
7b. **Support Agent** classifies sentiment (high anger), pulls shipment context, generates recommendation: "Apologize, offer ₱100 voucher + reschedule to tomorrow priority slot. Confidence 0.88. Note: SLA was actually met (delivery still within window)." Creates `EscalationCase`. *(Layer 3)*
7c. **Operator** sees escalation in queue (priority HIGH because customer sentiment is hostile + business value is moderate). Reads agent recommendation. Modifies it: "actually give them a full refund + voucher, customer is clearly going to churn otherwise." Clicks accept-with-modification.
7d. **System** processes refund + voucher under `actor=operator_123`, `agent_recommendation_id=...`, `supervision_layer=3`, `resolution=modified`.
7e. **Training pipeline** captures override: "for high-anger sentiment + repeat customer, agent should recommend full refund instead of voucher." Goes into next agent eval set.

**Operator actions in exception path: one.** Took 30 seconds. Required judgment the agent was not yet trusted to make. The override teaches the next version.

## Consequences

### Positive

- **Throughput is not bounded by human speed.** The agent handles the routine 95% autonomously; operators handle the exceptional 5% with full context pre-loaded.
- **Trust is earned, not assumed.** The competence envelope starts conservative and expands based on measured outcomes. Partners control the rate of expansion.
- **Audit is unified.** One log table, one query, one truth — regardless of whether the actor was human or AI.
- **Operator job becomes higher-leverage.** Operators stop being human routers and start being judgment specialists. Better for retention, better for hiring, better for the Partner's economics.
- **The agent improves continuously.** Every operator override is training data. The system gets smarter as it's used, not stale.
- **Failure modes are isolated.** A misbehaving agent is rolled back like any other service. A kill switch reverts that agent to Layer 5 (operator only) until the issue is resolved. The platform keeps running.
- **Sales pitch is honest.** Partners can see "5x operator leverage" measured in actual shift hours saved, not vendor marketing.

### Negative

- **The escalation queue is the new single point of failure.** If the queue fills up faster than operators can drain it, customers wait. Mitigation: SLO on queue depth + auto-shrink envelope when queue is overloaded (agent gets more conservative under pressure, paradoxically draining the queue faster).
- **Operators must be retrained.** Existing dispatchers used to manual workflows will resist. Mitigation: phased rollout per Partner, training program included in onboarding, new hires onboarded directly into the new model.
- **Per-action audit detail is heavy.** Every agent action logs reasoning trace + MCP call sequence. Storage cost is non-trivial. Mitigation: hot store for 90 days, cold store (S3 + Athena) thereafter.
- **The competence envelope is a config surface that can be misused.** A Partner admin who widens the envelope recklessly can cause incidents. Mitigation: envelope changes are versioned, audited, and gated behind a "I understand the risk" confirmation. Auto-shrink on bad outcomes provides a safety net.
- **The training pipeline introduces a feedback loop that can amplify operator bias.** If operators consistently override the agent in a biased way (e.g. always assigning shipments to their friend's driver), the next agent version learns the bias. Mitigation: bias monitoring on training data, periodic eval against a held-out neutral test set, fairness audits.

### Neutral

- **The line between "agent" and "system" blurs.** Some actions are taken by the agent based on event triggers (Kafka consumer), some are taken by deterministic code in the same service. The audit log distinguishes via `actor_type`. This is fine — the user does not need to care which one acted, as long as the audit is honest.
- **The platform tier owns more than CLAUDE.md currently shows.** The audit log, escalation queue, and training pipeline are platform-tier services not yet in the 18-service inventory. They will be added when the AI Layer is built in earnest (currently scaffolded but not in production).

## Alternatives Considered

### Alternative 1: Approval-gated agent (operator approves every action)

Agent generates suggestions, operator clicks approve/reject for each one.

**Rejected** because throughput collapses to human speed. The value proposition of Agentic SaaS dies. Acceptable for a pilot, not for production.

### Alternative 2: Fully autonomous agent (no operator in the loop)

Agent has full authority. Operator is informational only.

**Rejected** because:
- Trust is not earnable in a high-stakes domain (shipping money, contracts, customer relationships) without a human override path.
- Regulatory risk: GDPR and PDPA require human review of automated decisions affecting individuals.
- A single bad agent deploy could destroy a Partner's business overnight.
- Partners will refuse to adopt — the entire logistics market is risk-averse.

### Alternative 3: Confidence threshold only (single number, "act if > 0.9")

Replace the multi-dimensional competence envelope with a single confidence cutoff.

**Rejected** because:
- LLM confidence is poorly calibrated and varies by case type. A 0.9 on dispatch is not the same as 0.9 on refund.
- Risk and value are not captured. A high-confidence wrong assignment of a ₱500 shipment is fine; a high-confidence wrong assignment of a ₱500,000 cross-border shipment is a disaster.
- Reversibility is not captured. Some actions can be undone in 30 seconds; some cannot.
- The single threshold becomes a tuning nightmare with no clear semantics for stakeholders.

The competence envelope captures all four dimensions (case type, value, reversibility, confidence) explicitly.

### Alternative 4: Separate agent API and operator API

Build a parallel API surface for agents with relaxed RBAC.

**Rejected** because:
- Two RBAC systems = inevitable drift = security incident waiting to happen.
- Two audit trails = nothing reconciles = compliance failure.
- Encourages "give the agent superpowers because debugging RBAC is hard" — exactly the wrong incentive.
- Partners cannot reason about what the agent can do because the agent's authority lives in a different system than the operators they understand.

The single API surface forces honesty: the agent is a principal with bounded authority, just like every other principal.

## Migration Plan

### Phase 1 — Foundation (this sprint)

1. Add `actor_type` and agent metadata fields to the existing audit log table.
2. Create `escalation_cases` table at the platform tier.
3. Define the first three agents' competence envelopes in code (Dispatch, Support, Billing). Conservative defaults.
4. Build the operator escalation queue UI in the Admin Portal.

### Phase 2 — Wire the first agent (Dispatch)

1. Dispatch Agent reads from order-intake + dispatch + driver-ops via MCP (per ADR-0004).
2. Layer 1 actions (routine driver assignment) go live for one pilot Partner.
3. Layer 2 actions (auto-reschedule on missed delivery) go live with operator notification.
4. Layer 3 actions (high-value reassignment) generate escalation cases.
5. Measure: agent action rate, escalation queue depth, operator override rate, customer outcome metrics.

### Phase 3 — Operator override training pipeline

1. Kafka topic `logisticos.agent.override` published from the escalation case resolution flow.
2. MLOps consumer writes labeled examples to the training dataset, partitioned per Partner.
3. Weekly eval run against held-out test set + bias audit.
4. Manual decision to widen the dispatch envelope based on results.

### Phase 4 — Roll out remaining agents

Support Agent → Billing Agent → Marketing Agent → Operations Copilot, in that order. Each follows the same pattern: define envelope, wire MCP, ship Layer 1, then Layer 2, then Layer 3, then training loop.

### Phase 5 — Cross-product

When Carwash, Maintenance, MICE, Ride-Hailing, or Food Delivery come online (per ADR-0009), the same agents extend to those products with new MCP toolsets and product-specific envelopes. The platform-tier audit log, escalation queue, and training pipeline serve all products.

## References

- ADR-0004: MCP for AI Interoperability (every operational service exposes MCP tools for the agent)
- ADR-0008: Multi-Tenancy RLS Strategy (RLS extends to MCP layer; agent cannot cross tenant boundaries)
- ADR-0009: Multi-Product Platform Gateway Topology (AI Layer is platform-tier, serves all products)
- `project_agentic_directive.md` (memory): LogisticOS is Agentic As A Service. AI agents are first-class operators. Every service exposes MCP tools.
- CLAUDE.md → "AI Integration Standards": AI features are additive enhancements, model predictions logged for retraining, agent actions audited and reversible where possible — this ADR implements that mandate.
