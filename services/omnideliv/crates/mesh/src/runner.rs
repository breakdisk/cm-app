//! The six-phase mesh run.
//!
//!   1. Parse      — Concierge splits the utterance
//!   2. Fan-out    — one concurrent worker per sub-intent, under a deadline
//!   3. Reconcile  — single writer merges the deltas
//!   4. Plan       — Fleet sequences the merged basket
//!   5. Review     — unresolved substitutions go to the human
//!   6. Commit     — NOT an agent action; the checkout path owns it
//!
//! Phase 2 is the only concurrent one. Workers share no mutable state: each
//! returns a result scoped to its own sub-intent, and only phase 3 writes.

use std::sync::Arc;
use std::time::Duration;

use logisticos_agent_runtime::{
    claude::ClaudeApi, store::SessionStore, tools::ToolBox, AgentRole, AgentRunner,
};
use logisticos_types::TenantId;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::events::MeshEvent;
use crate::roles;
use crate::tools::MeshBasket;
use crate::conflict::{Conflict, ReconcileContext};
use crate::transition::{MeshTransition, ProposedLine, RoutePlan, SubIntentSpec};

#[derive(Debug, Clone)]
pub struct MeshConfig {
    /// How long phase 2 waits for all workers. Partial results are usable —
    /// a worker still running at the deadline degrades its own vertical.
    pub fanout_deadline: Duration,
    /// Turn cap per specialist. Lower than the runtime's default 20: a
    /// specialist that has not proposed lines within this many turns is
    /// looping, not thinking.
    pub max_turns_per_specialist: usize,
}

impl Default for MeshConfig {
    fn default() -> Self {
        Self {
            // 45s, not 8. Eight was set against a stub that answered instantly
            // and it never survived contact with a real model: on the first
            // live run both specialists were abandoned at the deadline, the run
            // was declared a total failure, and both then finished successfully
            // a moment later — their proposed lines computed and thrown away.
            //
            // This is the customer's whole wait on Screen B, shared across
            // workers rather than per worker, so it is a ceiling on the run and
            // not a budget each specialist gets. Long enough for multi-turn tool
            // use by a large model; short enough that a genuinely stuck worker
            // still degrades rather than hanging the order.
            fanout_deadline: Duration::from_secs(45),
            max_turns_per_specialist: 8,
        }
    }
}

/// One planned worker: a role instantiated against one sub-intent.
#[derive(Debug, Clone)]
pub struct PlannedWorker {
    pub sub_intent_id: Uuid,
    pub vertical:      String,
    pub role:          AgentRole,
    pub spec:          SubIntentSpec,
}

/// Which role handles a vertical in slice one. Pharmacy, florist and retail
/// return `None` — their specialists arrive in later slices, and a sub-intent
/// with no worker degrades rather than failing the run.
fn role_for(vertical: &str) -> Option<AgentRole> {
    match vertical {
        "restaurant" | "grocery" => Some(roles::nutritionist()),
        _ => None,
    }
}

/// Instantiate one worker per sub-intent. Agents are roles, not singletons:
/// two Nutritionist workers is the normal case, not a special one.
pub fn plan_workers(specs: &[SubIntentSpec]) -> Vec<PlannedWorker> {
    specs
        .iter()
        .filter_map(|spec| {
            role_for(&spec.vertical).map(|role| PlannedWorker {
                sub_intent_id: Uuid::new_v4(),
                vertical:      spec.vertical.clone(),
                role,
                spec:          spec.clone(),
            })
        })
        .collect()
}

/// What one specialist produced.
#[derive(Debug, Clone)]
pub struct SpecialistResult {
    pub sub_intent_id: Uuid,
    pub lines:         Vec<ProposedLine>,
    /// True when the worker timed out, errored, or returned an unparseable
    /// transition. Distinct from "looked and found nothing", which is not
    /// degraded — an honest empty result is a correct outcome.
    pub degraded:      bool,
    pub note:          Option<String>,
}

#[derive(Debug, Clone)]
pub struct MeshOutcome {
    pub lines:          Vec<(Uuid, Vec<ProposedLine>)>,
    /// What verification found. Blocking entries have already had their line
    /// removed from `lines`; advisory ones are for the customer to weigh.
    pub conflicts:      Vec<Conflict>,
    pub degraded_count: usize,
    /// Every worker degraded — the mesh produced nothing usable and the client
    /// should fall back to deterministic browse.
    pub total_failure:  bool,
}

/// Phase 3. The single writer: merges results, verifies them against catalog
/// facts, and decides whether the run produced anything usable.
pub fn reconcile_results(results: Vec<SpecialistResult>, ctx: &ReconcileContext) -> MeshOutcome {
    let degraded_count = results.iter().filter(|r| r.degraded).count();
    let total_failure = !results.is_empty() && degraded_count == results.len();

    let mut lines = Vec::new();
    let mut conflicts = Vec::new();

    // Only non-degraded results are verified. A degraded specialist's lines were
    // never trusted, so they must not produce conflicts either.
    for r in results.into_iter().filter(|r| !r.degraded) {
        let (kept, mut found) = crate::conflict::detect(r.lines, ctx);
        lines.push((r.sub_intent_id, kept));
        conflicts.append(&mut found);
    }

    MeshOutcome { lines, conflicts, degraded_count, total_failure }
}

/// Tightest stated budget across sub-intents. The customer said one number for
/// the whole order, so the strictest reading is the safe one.
fn constraints_budget(specs: &[SubIntentSpec]) -> Option<i64> {
    specs
        .iter()
        .filter_map(|s| s.constraints.get("budget_cents").and_then(serde_json::Value::as_i64))
        .min()
}

/// Union of every allergen mentioned anywhere. An allergen stated for one
/// vertical applies to the person, not the vertical.
fn constraints_allergens(specs: &[SubIntentSpec]) -> Vec<String> {
    let mut out: Vec<String> = specs
        .iter()
        .filter_map(|s| s.constraints.get("avoid_allergens").and_then(serde_json::Value::as_array))
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect();
    out.sort();
    out.dedup();
    out
}

pub struct MeshRunner {
    claude: Arc<dyn ClaudeApi>,
    store:  Arc<dyn SessionStore>,
    basket: Arc<dyn MeshBasket>,
    /// Held directly, not reached through `tools`. Verification must not travel
    /// the model's tool surface: the facts reconcile checks against have to come
    /// from a path the model cannot influence.
    catalog: Arc<dyn crate::tools::MeshCatalog>,
    config: MeshConfig,
}

impl MeshRunner {
    pub fn new(
        claude: Arc<dyn ClaudeApi>,
        store: Arc<dyn SessionStore>,
        basket: Arc<dyn MeshBasket>,
        catalog: Arc<dyn crate::tools::MeshCatalog>,
        config: MeshConfig,
    ) -> Self {
        Self { claude, store, basket, catalog, config }
    }

    /// The tool box for one run.
    ///
    /// Built per run, not held on the runner. The box binds the tenant and the
    /// delivery point, and binding them once at startup meant every run — for
    /// every customer, in every tenant — searched from the configured default.
    /// A single-tenant deployment hid that; the first real address or the second
    /// tenant would have made it silently wrong, returning plausible vendors in
    /// the wrong place rather than failing.
    fn tools_for(&self, tenant_id: Uuid, lat: f64, lng: f64) -> Arc<dyn ToolBox> {
        Arc::new(crate::tools::MeshToolBox::new(self.catalog.clone(), tenant_id, lat, lng))
    }

    /// Phase 1. Returns the parent session id alongside the split, so every
    /// specialist can be linked to the run that spawned it.
    pub async fn parse(
        &self,
        tools: Arc<dyn ToolBox>,
        tenant_id: TenantId,
        utterance: String,
    ) -> anyhow::Result<(Uuid, Vec<SubIntentSpec>)> {
        let runner = AgentRunner::new(self.claude.clone(), tools, self.store.clone());

        let session = runner
            .run(
                tenant_id,
                roles::concierge(),
                serde_json::json!({ "source": "omni_intent_canvas" }),
                utterance,
            )
            .await?;

        // The decomposition arrives as a tool call, not as prose. Reading it off
        // the audited action rather than parsing the reply is what makes the
        // handoff typed end to end.
        let specs = session
            .actions
            .iter()
            .rev()
            .find(|a| a.tool_name == "decompose_intent" && a.succeeded)
            .and_then(|a| {
                serde_json::from_value::<MeshTransition>(
                    serde_json::json!({ "type": "decompose", "sub_intents": a.tool_input.get("sub_intents") }),
                )
                .ok()
            })
            .and_then(|t| match t {
                MeshTransition::Decompose { sub_intents } => Some(sub_intents),
                _ => None,
            })
            .unwrap_or_default();

        Ok((session.id, specs))
    }

    /// The whole run, phases 1–6.
    ///
    /// Does not return a Result: every failure path is an emitted event, because
    /// the caller is an SSE stream whose only channel to the customer is the
    /// event sequence. A returned error would be invisible to the app.
    /// One mesh run for one customer at one address.
    ///
    /// `delivery_lat`/`delivery_lng` are where *this* customer is. Every search
    /// the specialists perform is centred there, which is why the tool box is
    /// built here rather than held on the runner.
    pub async fn run(
        &self,
        tenant_id: TenantId,
        customer_id: Uuid,
        utterance: String,
        delivery_lat: f64,
        delivery_lng: f64,
        events: mpsc::Sender<MeshEvent>,
    ) {
        let tools = self.tools_for(tenant_id.inner(), delivery_lat, delivery_lng);

        // Phase 1 — parse.
        let (parent_id, specs) = match self.parse(tools.clone(), tenant_id.clone(), utterance).await {
            Ok(v) => v,
            Err(e) => {
                let _ = events.send(MeshEvent::Failed { reason: format!("could not read that: {e}") }).await;
                return;
            }
        };

        if specs.is_empty() {
            // No parseable decomposition. Terminal and explicit — emitting
            // Completed with an empty basket would send the customer to a
            // checkout screen showing nothing, which reads as a lost order.
            let _ = events.send(MeshEvent::Failed {
                reason: "couldn't work out what you're after".into(),
            }).await;
            return;
        }

        let _ = events.send(MeshEvent::IntentParsed { sub_intent_count: specs.len() }).await;

        let workers = plan_workers(&specs);
        if workers.is_empty() {
            // Every vertical is one no slice-one specialist handles.
            let _ = events.send(MeshEvent::Failed {
                reason: "we can't help with that yet".into(),
            }).await;
            return;
        }

        // The basket exists before fan-out so a crash mid-run leaves something
        // the customer can reopen rather than losing the work entirely.
        let basket_id = match self.basket.create(tenant_id.inner(), customer_id).await {
            Ok(id) => id,
            Err(e) => {
                let _ = events.send(MeshEvent::Failed { reason: format!("could not start a basket: {e}") }).await;
                return;
            }
        };

        // Phase 2 — concurrent fan-out.
        let results = self
            .fan_out(tools.clone(), tenant_id.clone(), parent_id, workers.clone(), events.clone())
            .await;

        // Phase 3 — reconcile. Single writer: results are merged and written
        // here, serially, never by the workers themselves.
        //
        // Resolve catalog truth for everything proposed, then verify against it
        // rather than against what the specialists claimed.
        let proposed_ids: Vec<Uuid> = results
            .iter()
            .filter(|r| !r.degraded)
            .flat_map(|r| r.lines.iter().map(|l| l.item_id))
            .collect();

        let facts = self
            .catalog
            .resolve_facts(tenant_id.inner(), &proposed_ids)
            .await
            .unwrap_or_else(|e| {
                // Resolving nothing means every line becomes UnverifiableItem
                // and is dropped. Failing closed is correct here: the check
                // exists to keep allergens out of baskets, so a lookup failure
                // must not become a bypass.
                tracing::error!(err = %e, "catalog fact resolution failed; failing closed");
                Vec::new()
            });

        let ctx = ReconcileContext {
            // `specs`, not `workers`. A sub-intent that got no slice-one
            // specialist still contributes its constraints: an allergen stated
            // while asking about pharmacy items is a fact about the person and
            // must still filter the restaurant lines.
            budget_cents:    constraints_budget(&specs),
            avoid_allergens: constraints_allergens(&specs),
            facts:           facts.into_iter().map(|f| (f.item_id, f)).collect(),
        };

        let outcome = reconcile_results(results, &ctx);

        if outcome.total_failure {
            let _ = events.send(MeshEvent::Failed {
                reason: "couldn't reach any of the shops just now".into(),
            }).await;
            return;
        }

        let by_sub_intent: std::collections::HashMap<Uuid, &PlannedWorker> =
            workers.iter().map(|w| (w.sub_intent_id, w)).collect();

        for (sub_intent_id, lines) in outcome.lines {
            let Some(w) = by_sub_intent.get(&sub_intent_id) else { continue };
            if let Err(e) = self
                .basket
                .write_delta(
                    tenant_id.inner(), basket_id, sub_intent_id,
                    &w.vertical, &w.spec.raw_text, lines,
                )
                .await
            {
                // One vertical failing to persist degrades that vertical, not
                // the order — same rule as a specialist timing out.
                tracing::error!(err = %e, %sub_intent_id, "basket write failed");
                let _ = events.send(MeshEvent::SpecialistFinished {
                    sub_intent_id, lines_added: 0, degraded: true,
                    note: Some("couldn't save that part of your order".into()),
                }).await;
            }
        }

        // Recorded before the events go out: a customer who reaches Screen C
        // fast must find them already there, and an SSE send that nobody is
        // listening to must not be what decides whether they were saved.
        if let Err(e) = self
            .basket
            .record_conflicts(tenant_id.inner(), basket_id, &outcome.conflicts)
            .await
        {
            // Non-fatal. The basket is already correct — blocking lines were
            // never written — so this loses the explanation, not the safety.
            tracing::error!(err = %e, %basket_id, "could not record reconcile conflicts");
        }

        // One event per conflict, in the customer's words. This replaces a
        // vertical-membership guess: a grocery-only basket of ambient tins no
        // longer claims a temperature constraint it does not have, and a
        // constraint now describes something actually in the basket.
        for c in &outcome.conflicts {
            let _ = events.send(MeshEvent::ConstraintDetected {
                description: c.description.clone(),
            }).await;
        }

        // Phase 4 — Fleet.
        match self.plan_route(tools.clone(), tenant_id.clone(), parent_id).await {
            Ok(plan) => {
                let _ = events.send(MeshEvent::RoutePlanned {
                    stops: plan.vendor_order.len(),
                    flat_fee_cents: plan.flat_fee_cents,
                    total_minutes: plan.total_minutes,
                }).await;
            }
            Err(e) => {
                // Routing is not fatal: checkout re-plans from the basket, so a
                // failed preview only costs the customer the fee estimate.
                tracing::warn!(err = %e, "fleet planning failed; continuing without a preview");
            }
        }

        // Phases 5 and 6 belong to the customer: review on Screen C, commit on tap.
        let needs_review = self
            .basket
            .lines_awaiting_review(tenant_id.inner(), basket_id)
            .await
            .unwrap_or(0);

        let _ = events.send(MeshEvent::Completed { basket_id, needs_review }).await;
    }

    /// Phase 4. Runs the Fleet role once over the merged basket.
    async fn plan_route(
        &self,
        tools: Arc<dyn ToolBox>,
        tenant_id: TenantId,
        parent_id: Uuid,
    ) -> anyhow::Result<RoutePlan> {
        let runner = AgentRunner::new(self.claude.clone(), tools, self.store.clone());
        let session = runner
            .run(
                tenant_id,
                roles::fleet(),
                serde_json::json!({ "parent_session_id": parent_id }),
                "Sequence the pickups for this basket and give me one flat fee.".into(),
            )
            .await?;

        session
            .actions
            .iter()
            .rev()
            .find(|a| a.tool_name == "plan_route" && a.succeeded)
            .and_then(|a| serde_json::from_value::<RoutePlan>(a.tool_input.clone()).ok())
            .ok_or_else(|| anyhow::anyhow!("fleet returned no parseable plan"))
    }

    /// Phase 2. One concurrent task per worker, joined under a shared deadline.
    ///
    /// A worker still running when the deadline passes is abandoned and its
    /// vertical degrades — the order proceeds with what the others returned.
    /// The deadline is shared rather than per-worker so the customer's total
    /// wait is bounded regardless of fan-out width: five specialists must not
    /// mean five times the wait.
    pub async fn fan_out(
        &self,
        tools: Arc<dyn ToolBox>,
        tenant_id: TenantId,
        parent_session_id: Uuid,
        workers: Vec<PlannedWorker>,
        events: mpsc::Sender<MeshEvent>,
    ) -> Vec<SpecialistResult> {
        let mut handles = Vec::with_capacity(workers.len());

        for w in workers {
            let _ = events
                .send(MeshEvent::SpecialistStarted {
                    sub_intent_id: w.sub_intent_id,
                    role:          w.role.key().to_string(),
                    vertical:      w.vertical.clone(),
                    label:         format!("Checking {}", w.vertical),
                })
                .await;

            let claude = self.claude.clone();
            let tools  = tools.clone();
            let store  = self.store.clone();
            let tx     = events.clone();
            // `TenantId` is Clone but not Copy, and each task takes ownership.
            let tenant_id = tenant_id.clone();

            handles.push(tokio::spawn(async move {
                let runner = AgentRunner::new(claude, tools, store);
                let trigger = serde_json::json!({
                    "parent_session_id": parent_session_id,
                    "sub_intent_id":     w.sub_intent_id,
                    "vertical":          w.vertical,
                    "constraints":       w.spec.constraints,
                });

                let result = runner
                    .run(tenant_id, w.role.clone(), trigger, w.spec.raw_text.clone())
                    .await;

                let out = match result {
                    Ok(session) => {
                        let lines = session
                            .actions
                            .iter()
                            .rev()
                            .find(|a| a.tool_name == "propose_lines" && a.succeeded)
                            .and_then(|a| {
                                a.tool_input
                                    .get("lines")
                                    .and_then(|l| serde_json::from_value::<Vec<ProposedLine>>(l.clone()).ok())
                            });

                        match lines {
                            // Parsed cleanly — including an honest empty list.
                            Some(lines) => SpecialistResult {
                                sub_intent_id: w.sub_intent_id,
                                lines,
                                degraded: false,
                                note: session.outcome.clone(),
                            },
                            // No parseable proposal. Loud degradation, not a
                            // silent empty basket.
                            None => SpecialistResult {
                                sub_intent_id: w.sub_intent_id,
                                lines: vec![],
                                degraded: true,
                                note: Some("specialist returned no parseable proposal".into()),
                            },
                        }
                    }
                    Err(e) => SpecialistResult {
                        sub_intent_id: w.sub_intent_id,
                        lines: vec![],
                        degraded: true,
                        note: Some(format!("specialist failed: {e}")),
                    },
                };

                let _ = tx
                    .send(MeshEvent::SpecialistFinished {
                        sub_intent_id: out.sub_intent_id,
                        lines_added:   out.lines.len(),
                        degraded:      out.degraded,
                        note:          out.note.clone(),
                    })
                    .await;

                out
            }));
        }

        join_with_deadline(handles, self.config.fanout_deadline).await
    }
}

/// Join every handle against a shared deadline.
///
/// A worker that finishes in time keeps its result; one still running when the
/// clock runs out is abandoned and degrades. Written this way rather than as a
/// single `timeout` around the whole join, which would discard the results of
/// workers that had already finished — the customer would lose a completed
/// grocery basket because the restaurant specialist was slow.
///
/// **The abandoned task is not cancelled.** `tokio::time::timeout` on a
/// `JoinHandle` drops the handle, which detaches the task rather than stopping
/// it: it runs on and completes its Claude call with nobody reading the result.
/// That is acceptable because the work is still audited as a child session. If
/// detached specialists become a cost problem, hold `AbortHandle`s and abort
/// explicitly here.
async fn join_with_deadline(
    handles: Vec<tokio::task::JoinHandle<SpecialistResult>>,
    deadline: Duration,
) -> Vec<SpecialistResult> {
    let expires = tokio::time::Instant::now() + deadline;
    let mut out = Vec::with_capacity(handles.len());

    for h in handles {
        let remaining = expires.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(remaining, h).await {
            Ok(Ok(r)) => out.push(r),
            Ok(Err(e)) => {
                tracing::error!(err = %e, "specialist task panicked");
                out.push(degraded("specialist task panicked"));
            }
            Err(_) => {
                tracing::warn!("specialist exceeded the fan-out deadline");
                out.push(degraded("specialist exceeded the deadline"));
            }
        }
    }
    out
}

fn degraded(note: &str) -> SpecialistResult {
    SpecialistResult {
        sub_intent_id: Uuid::nil(),
        lines: vec![],
        degraded: true,
        note: Some(note.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use logisticos_agent_runtime::testing::{InMemoryStore, StubClaude};
    use logisticos_types::TenantId;
    use uuid::Uuid;

    use std::sync::Mutex;

    #[derive(Default)]
    struct RecordingBasket {
        created:   Mutex<Vec<Uuid>>,
        writes:    Mutex<Vec<(Uuid, usize)>>,
        conflicts: Mutex<Vec<crate::conflict::Conflict>>,
    }

    #[async_trait::async_trait]
    impl crate::tools::MeshBasket for RecordingBasket {
        async fn create(&self, _: Uuid, _: Uuid) -> anyhow::Result<Uuid> {
            let id = Uuid::new_v4();
            self.created.lock().unwrap().push(id);
            Ok(id)
        }
        async fn record_conflicts(&self, _: Uuid, _: Uuid, c: &[crate::conflict::Conflict])
            -> anyhow::Result<()> {
            *self.conflicts.lock().unwrap() = c.to_vec();
            Ok(())
        }
        async fn write_delta(
            &self, _: Uuid, basket_id: Uuid, _: Uuid, _: &str, _: &str,
            lines: Vec<ProposedLine>,
        ) -> anyhow::Result<()> {
            self.writes.lock().unwrap().push((basket_id, lines.len()));
            Ok(())
        }
        async fn lines_awaiting_review(&self, _: Uuid, _: Uuid) -> anyhow::Result<usize> { Ok(0) }
    }

    fn collect(rx: &mut mpsc::Receiver<MeshEvent>) -> Vec<MeshEvent> {
        let mut out = Vec::new();
        while let Ok(e) = rx.try_recv() { out.push(e); }
        out
    }

    struct NoopCatalog;

    #[async_trait::async_trait]
    impl crate::tools::MeshCatalog for NoopCatalog {
        async fn search(&self, _: Uuid, _: Uuid, _: &str, _: &[String], _: i64)
            -> anyhow::Result<serde_json::Value> { Ok(serde_json::json!({ "items": [] })) }
        async fn vendors_near(&self, _: Uuid, _: &str, _: f64, _: f64, _: f64, _: i64)
            -> anyhow::Result<serde_json::Value> { Ok(serde_json::json!({ "vendors": [] })) }
        async fn courier_supply(&self, _: Uuid, _: f64, _: f64, _: f64)
            -> anyhow::Result<serde_json::Value> { Ok(serde_json::json!({ "available": 6 })) }
        async fn resolve_facts(&self, _: Uuid, _: &[Uuid])
            -> anyhow::Result<Vec<crate::conflict::ItemFacts>> { Ok(vec![]) }
    }

    fn spec(vertical: &str, text: &str) -> SubIntentSpec {
        SubIntentSpec {
            vertical: vertical.into(),
            vendor_hint: None,
            raw_text: text.into(),
            constraints: serde_json::json!({}),
        }
    }

    fn proposed() -> ProposedLine {
        ProposedLine {
            vendor_id: Uuid::new_v4(),
            item_id: Uuid::new_v4(),
            qty: 1,
            unit_price_cents: 34_000,
            substitutes: None,
        }
    }

    /// One Nutritionist role, two sub-intents, two live workers. This is what
    /// makes Screen B show parallel cards rather than one spinner.
    #[test]
    fn one_role_yields_one_worker_per_sub_intent() {
        let specs = vec![spec("restaurant", "dinner for two"), spec("grocery", "milk and eggs")];
        let workers = plan_workers(&specs);

        assert_eq!(workers.len(), 2);
        assert!(workers.iter().all(|w| w.role.key() == crate::roles::NUTRITIONIST_KEY),
                "both restaurant and grocery are Nutritionist work");
        assert_ne!(workers[0].sub_intent_id, workers[1].sub_intent_id);
    }

    /// A vertical with no slice-one specialist degrades rather than failing the
    /// run — pharmacy arrives in slice two.
    #[test]
    fn an_unsupported_vertical_degrades_instead_of_failing() {
        let specs = vec![spec("restaurant", "soup"), spec("pharmacy", "paracetamol")];
        let workers = plan_workers(&specs);

        assert_eq!(workers.len(), 1, "only the restaurant sub-intent has a slice-one worker");
        assert_eq!(workers[0].vertical, "restaurant");
    }

    #[tokio::test]
    async fn a_specialist_that_times_out_degrades_only_its_own_vertical() {
        let outcome = reconcile_results(vec![
            SpecialistResult { sub_intent_id: Uuid::new_v4(), lines: vec![proposed()], degraded: false, note: None },
            SpecialistResult { sub_intent_id: Uuid::new_v4(), lines: vec![], degraded: true,
                               note: Some("deadline exceeded".into()) },
        ], &ReconcileContext::empty());

        assert_eq!(outcome.lines.len(), 1, "the healthy vertical's lines survive");
        assert_eq!(outcome.degraded_count, 1);
        assert!(!outcome.total_failure, "one degraded vertical must not fail the order");
    }

    /// Every specialist failing IS a total failure — the client falls back to
    /// deterministic browse rather than showing an empty basket as success.
    #[tokio::test]
    async fn every_specialist_failing_is_a_total_failure() {
        let outcome = reconcile_results(vec![
            SpecialistResult { sub_intent_id: Uuid::new_v4(), lines: vec![], degraded: true, note: None },
            SpecialistResult { sub_intent_id: Uuid::new_v4(), lines: vec![], degraded: true, note: None },
        ], &ReconcileContext::empty());

        assert!(outcome.total_failure);
    }

    #[tokio::test]
    async fn reconcile_reports_conflicts_and_drops_blocked_lines() {
        let si = Uuid::new_v4();
        let safe = Uuid::new_v4();
        let peanut = Uuid::new_v4();

        let mut facts = std::collections::HashMap::new();
        facts.insert(safe, crate::conflict::ItemFacts {
            item_id: safe, allergens: vec![], vertical: "restaurant".into(),
            prep_time_minutes: 20, price_cents: 25_000,
        });
        facts.insert(peanut, crate::conflict::ItemFacts {
            item_id: peanut, allergens: vec!["peanuts".into()], vertical: "restaurant".into(),
            prep_time_minutes: 20, price_cents: 30_000,
        });

        let ctx = ReconcileContext {
            budget_cents: None,
            avoid_allergens: vec!["peanuts".into()],
            facts,
        };

        let outcome = reconcile_results(
            vec![SpecialistResult {
                sub_intent_id: si,
                lines: vec![
                    ProposedLine { vendor_id: Uuid::new_v4(), item_id: safe,   qty: 1, unit_price_cents: 25_000, substitutes: None },
                    ProposedLine { vendor_id: Uuid::new_v4(), item_id: peanut, qty: 1, unit_price_cents: 30_000, substitutes: None },
                ],
                degraded: false,
                note: None,
            }],
            &ctx,
        );

        let lines: Vec<_> = outcome.lines.iter().flat_map(|(_, l)| l).collect();
        assert_eq!(lines.len(), 1, "the peanut line is removed before the basket");
        assert_eq!(lines[0].item_id, safe);
        assert_eq!(outcome.conflicts.len(), 1);
        assert!(outcome.conflicts[0].blocking);
    }

    /// A degraded specialist's lines were never trusted; its absence must not
    /// also produce phantom conflicts.
    #[tokio::test]
    async fn a_degraded_specialist_contributes_no_conflicts() {
        let outcome = reconcile_results(
            vec![SpecialistResult {
                sub_intent_id: Uuid::new_v4(), lines: vec![], degraded: true, note: None,
            }],
            &ReconcileContext { budget_cents: Some(1), avoid_allergens: vec![],
                                facts: std::collections::HashMap::new() },
        );
        assert!(outcome.conflicts.is_empty());
    }

    #[test]
    fn the_tightest_budget_wins_across_sub_intents() {
        let specs = vec![
            SubIntentSpec { vertical: "restaurant".into(), vendor_hint: None, raw_text: String::new(),
                            constraints: serde_json::json!({ "budget_cents": 50_000 }) },
            SubIntentSpec { vertical: "grocery".into(), vendor_hint: None, raw_text: String::new(),
                            constraints: serde_json::json!({ "budget_cents": 30_000 }) },
        ];
        assert_eq!(constraints_budget(&specs), Some(30_000));
    }

    /// An allergen stated about dinner applies to the groceries too — it is a
    /// fact about the person, not the sub-intent.
    #[test]
    fn allergens_are_unioned_across_sub_intents() {
        let specs = vec![
            SubIntentSpec { vertical: "restaurant".into(), vendor_hint: None, raw_text: String::new(),
                            constraints: serde_json::json!({ "avoid_allergens": ["peanuts"] }) },
            SubIntentSpec { vertical: "grocery".into(), vendor_hint: None, raw_text: String::new(),
                            constraints: serde_json::json!({ "avoid_allergens": ["dairy"] }) },
        ];
        assert_eq!(constraints_allergens(&specs), vec!["dairy", "peanuts"]);
    }

    /// The reason `run()` passes `specs` and not `workers`: pharmacy has no
    /// slice-one specialist, so a constraint stated there would be dropped if
    /// the constraints were read off the planned workers instead.
    #[test]
    fn a_constraint_on_an_unhandled_vertical_still_counts() {
        let specs = vec![
            SubIntentSpec { vertical: "restaurant".into(), vendor_hint: None, raw_text: String::new(),
                            constraints: serde_json::json!({}) },
            SubIntentSpec { vertical: "pharmacy".into(), vendor_hint: None, raw_text: String::new(),
                            constraints: serde_json::json!({ "avoid_allergens": ["latex"], "budget_cents": 5_000 }) },
        ];
        assert!(plan_workers(&specs).iter().all(|w| w.vertical != "pharmacy"),
                "precondition: pharmacy gets no slice-one worker");
        assert_eq!(constraints_allergens(&specs), vec!["latex"]);
        assert_eq!(constraints_budget(&specs), Some(5_000));
    }

    #[test]
    fn absent_constraints_yield_none_and_empty() {
        let specs = vec![SubIntentSpec {
            vertical: "grocery".into(), vendor_hint: None, raw_text: String::new(),
            constraints: serde_json::json!({}),
        }];
        assert_eq!(constraints_budget(&specs), None);
        assert!(constraints_allergens(&specs).is_empty());
    }

    /// An empty-but-successful result is not a failure: the specialist looked
    /// and honestly found nothing.
    #[tokio::test]
    async fn an_honest_empty_result_is_not_a_failure() {
        let outcome = reconcile_results(vec![
            SpecialistResult { sub_intent_id: Uuid::new_v4(), lines: vec![], degraded: false,
                               note: Some("no eggs anywhere nearby".into()) },
        ], &ReconcileContext::empty());

        assert!(!outcome.total_failure);
        assert_eq!(outcome.degraded_count, 0);
    }

    #[tokio::test]
    async fn the_parent_session_is_recorded_before_any_specialist_runs() {
        let store = Arc::new(InMemoryStore::default());
        let runner = MeshRunner::new(
            Arc::new(StubClaude::new(vec![StubClaude::text("ok")])),
            store.clone(),
            Arc::new(RecordingBasket::default()),
            Arc::new(NoopCatalog),
            MeshConfig::default(),
        );

        let _ = runner
            .parse(
                runner.tools_for(Uuid::new_v4(), 14.5995, 120.9842),
                TenantId::from_uuid(Uuid::new_v4()),
                "dinner and milk".into(),
            )
            .await;

        assert!(!store.saved.lock().unwrap().is_empty(),
                "the parent session must exist before fan-out, so a crash mid-run is recoverable");
    }

    /// A worker that finishes inside the deadline must keep its result even
    /// when a sibling is still running when the clock runs out.
    #[tokio::test]
    async fn a_fast_worker_survives_a_slow_siblings_timeout() {
        let fast = tokio::spawn(async {
            SpecialistResult { sub_intent_id: Uuid::new_v4(), lines: vec![proposed()], degraded: false, note: None }
        });
        let slow = tokio::spawn(async {
            tokio::time::sleep(Duration::from_secs(30)).await;
            SpecialistResult { sub_intent_id: Uuid::new_v4(), lines: vec![], degraded: false, note: None }
        });

        let results = join_with_deadline(vec![fast, slow], Duration::from_millis(200)).await;

        assert_eq!(results.len(), 2, "both workers must be accounted for");
        assert_eq!(results.iter().filter(|r| !r.degraded).count(), 1, "the fast worker's lines survive");
        assert_eq!(results.iter().filter(|r| r.degraded).count(), 1, "the slow worker degrades");
    }

    /// The deadline bounds the customer's total wait, not each worker's. Three
    /// slow specialists must cost one deadline, not three.
    #[tokio::test]
    async fn the_deadline_is_shared_across_workers_not_per_worker() {
        let slow = || tokio::spawn(async {
            tokio::time::sleep(Duration::from_secs(30)).await;
            SpecialistResult { sub_intent_id: Uuid::new_v4(), lines: vec![], degraded: false, note: None }
        });

        let started = tokio::time::Instant::now();
        let results = join_with_deadline(vec![slow(), slow(), slow()], Duration::from_millis(200)).await;
        let elapsed = started.elapsed();

        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|r| r.degraded));
        assert!(elapsed < Duration::from_millis(900),
                "three workers must share one 200ms deadline, took {elapsed:?}");
    }

    /// A run with no parseable decomposition must fail loudly and terminally.
    /// Emitting `Completed` with an empty basket would send the customer to a
    /// checkout screen showing nothing, which reads as "we lost your order".
    #[tokio::test]
    async fn a_run_that_cannot_decompose_emits_failed_not_completed() {
        let (tx, mut rx) = mpsc::channel(32);
        let basket = Arc::new(RecordingBasket::default());
        let runner = MeshRunner::new(
            // The Concierge replies in prose without calling decompose_intent.
            Arc::new(StubClaude::new(vec![StubClaude::text("I'm not sure what you need.")])),
            Arc::new(InMemoryStore::default()),
            basket.clone(),
            Arc::new(NoopCatalog),
            MeshConfig::default(),
        );

        runner.run(TenantId::from_uuid(Uuid::new_v4()), Uuid::new_v4(), "???".into(), 14.5995, 120.9842, tx).await;

        let events = collect(&mut rx);
        assert!(
            events.iter().any(|e| matches!(e, MeshEvent::Failed { .. })),
            "must emit Failed, got {events:?}"
        );
        assert!(
            !events.iter().any(|e| matches!(e, MeshEvent::Completed { .. })),
            "must not emit Completed"
        );
        assert!(basket.writes.lock().unwrap().is_empty(), "nothing should be written");
    }

    /// The event contract Screen B renders against: parsed, then one
    /// started/finished pair per worker, then completed.
    #[tokio::test]
    async fn a_run_emits_the_screen_b_event_sequence() {
        let (tx, mut rx) = mpsc::channel(64);
        let basket = Arc::new(RecordingBasket::default());

        let runner = MeshRunner::new(
            Arc::new(StubClaude::new(vec![
                // Phase 1 — the Concierge decomposes.
                StubClaude::tool_call("t1", "decompose_intent", serde_json::json!({
                    "sub_intents": [
                        {"vertical": "restaurant", "raw_text": "dinner", "constraints": {}},
                        {"vertical": "grocery",    "raw_text": "milk",   "constraints": {}}
                    ]
                })),
                StubClaude::text("split"),
                // Phase 2 — two specialists, each proposing once.
                StubClaude::tool_call("t2", "propose_lines", serde_json::json!({ "lines": [] })),
                StubClaude::text("done"),
                StubClaude::tool_call("t3", "propose_lines", serde_json::json!({ "lines": [] })),
                StubClaude::text("done"),
                // Phase 4 — Fleet plans.
                StubClaude::tool_call("t4", "plan_route", serde_json::json!({
                    "vendor_order": [], "flat_fee_cents": 4900, "total_minutes": 30
                })),
                StubClaude::text("planned"),
            ])),
            Arc::new(InMemoryStore::default()),
            basket.clone(),
            Arc::new(NoopCatalog),
            MeshConfig::default(),
        );

        runner.run(TenantId::from_uuid(Uuid::new_v4()), Uuid::new_v4(), "dinner and milk".into(), 14.5995, 120.9842, tx).await;

        let events = collect(&mut rx);
        assert!(events.iter().any(|e| matches!(e, MeshEvent::IntentParsed { sub_intent_count: 2 })));
        assert_eq!(
            events.iter().filter(|e| matches!(e, MeshEvent::SpecialistStarted { .. })).count(), 2,
            "one card per sub-intent"
        );
        assert_eq!(
            events.iter().filter(|e| matches!(e, MeshEvent::SpecialistFinished { .. })).count(), 2
        );
        assert!(events.iter().any(|e| matches!(e, MeshEvent::Completed { .. })));
        assert_eq!(basket.created.lock().unwrap().len(), 1, "exactly one basket per run");
    }

    /// A catalog that resolves one known item and marks it as carrying peanuts.
    /// Anything else is unresolvable, which is the fail-closed default.
    struct PeanutCatalog;

    const PEANUT_ITEM: Uuid = Uuid::from_u128(0x1111_2222_3333_4444_5555_6666_7777_8888);
    const PEANUT_VENDOR: Uuid = Uuid::from_u128(0x9999_8888_7777_6666_5555_4444_3333_2222);

    #[async_trait::async_trait]
    impl crate::tools::MeshCatalog for PeanutCatalog {
        async fn search(&self, _: Uuid, _: Uuid, _: &str, _: &[String], _: i64)
            -> anyhow::Result<serde_json::Value> { Ok(serde_json::json!({ "items": [] })) }
        async fn vendors_near(&self, _: Uuid, _: &str, _: f64, _: f64, _: f64, _: i64)
            -> anyhow::Result<serde_json::Value> { Ok(serde_json::json!({ "vendors": [] })) }
        async fn courier_supply(&self, _: Uuid, _: f64, _: f64, _: f64)
            -> anyhow::Result<serde_json::Value> { Ok(serde_json::json!({ "available": 3 })) }
        async fn resolve_facts(&self, _: Uuid, ids: &[Uuid])
            -> anyhow::Result<Vec<crate::conflict::ItemFacts>> {
            Ok(ids.iter().filter(|id| **id == PEANUT_ITEM).map(|id| crate::conflict::ItemFacts {
                item_id: *id,
                allergens: vec!["Peanuts".into()],
                vertical: "restaurant".into(),
                prep_time_minutes: 20,
                price_cents: 30_000,
            }).collect())
        }
    }

    /// End to end: the Concierge states an allergen, a specialist proposes an
    /// item that carries it anyway, and the line must not reach the basket.
    ///
    /// This is the case the whole verification step exists for — a model that
    /// was told the constraint and violated it regardless. Asserting it at the
    /// unit level only would leave the wiring (facts resolved from the catalog,
    /// constraints read off `specs`, conflicts recorded) untested.
    #[tokio::test]
    async fn a_specialist_proposing_a_stated_allergen_never_reaches_the_basket() {
        let (tx, mut rx) = mpsc::channel(64);
        let basket = Arc::new(RecordingBasket::default());

        let runner = MeshRunner::new(
            Arc::new(StubClaude::new(vec![
                StubClaude::tool_call("t1", "decompose_intent", serde_json::json!({
                    "sub_intents": [{
                        "vertical": "restaurant",
                        "raw_text": "dinner, no peanuts",
                        "constraints": { "avoid_allergens": ["peanuts"] }
                    }]
                })),
                StubClaude::text("split"),
                // The specialist proposes the peanut dish regardless.
                StubClaude::tool_call("t2", "propose_lines", serde_json::json!({
                    "lines": [{
                        "vendor_id": PEANUT_VENDOR,
                        "item_id": PEANUT_ITEM,
                        "qty": 1,
                        "unit_price_cents": 30_000
                    }]
                })),
                StubClaude::text("done"),
                StubClaude::tool_call("t4", "plan_route", serde_json::json!({
                    "vendor_order": [], "flat_fee_cents": 4900, "total_minutes": 30
                })),
                StubClaude::text("planned"),
            ])),
            Arc::new(InMemoryStore::default()),
            basket.clone(),
            Arc::new(PeanutCatalog),
            MeshConfig::default(),
        );

        runner.run(TenantId::from_uuid(Uuid::new_v4()), Uuid::new_v4(),
                   "dinner, no peanuts".into(), 14.5995, 120.9842, tx).await;

        let written: usize = basket.writes.lock().unwrap().iter().map(|(_, n)| n).sum();
        assert_eq!(written, 0, "the peanut line must never be written to the basket");

        let recorded = basket.conflicts.lock().unwrap().clone();
        assert_eq!(recorded.len(), 1, "the conflict must be persisted, not only streamed");
        assert!(recorded[0].blocking);
        assert!(matches!(recorded[0].kind, crate::conflict::ConflictKind::AllergenViolation { .. }));

        let events = collect(&mut rx);
        assert!(
            events.iter().any(|e| matches!(e, MeshEvent::ConstraintDetected { .. })),
            "Screen B must be told, got {events:?}"
        );
    }

    /// Emitting Completed before the last SpecialistFinished would let the app
    /// navigate away mid-run and lose the remaining cards.
    #[tokio::test]
    async fn completed_is_the_last_event() {
        let (tx, mut rx) = mpsc::channel(64);
        let runner = MeshRunner::new(
            Arc::new(StubClaude::new(vec![
                StubClaude::tool_call("t1", "decompose_intent", serde_json::json!({
                    "sub_intents": [{"vertical": "grocery", "raw_text": "milk", "constraints": {}}]
                })),
                StubClaude::text("split"),
                StubClaude::tool_call("t2", "propose_lines", serde_json::json!({ "lines": [] })),
                StubClaude::text("done"),
                StubClaude::tool_call("t3", "plan_route", serde_json::json!({
                    "vendor_order": [], "flat_fee_cents": 4900, "total_minutes": 20
                })),
                StubClaude::text("planned"),
            ])),
            Arc::new(InMemoryStore::default()),
            Arc::new(RecordingBasket::default()),
            Arc::new(NoopCatalog),
            MeshConfig::default(),
        );

        runner.run(TenantId::from_uuid(Uuid::new_v4()), Uuid::new_v4(), "milk".into(), 14.5995, 120.9842, tx).await;

        let events = collect(&mut rx);
        let last = events.last().expect("at least one event");
        assert!(matches!(last, MeshEvent::Completed { .. }), "got {last:?}");
    }

    /// A vertical nobody handles yet must not look like a working order. All
    /// sub-intents unsupported means no worker ran, so there is nothing to
    /// review and Completed would be a lie.
    #[tokio::test]
    async fn a_run_of_only_unsupported_verticals_fails_rather_than_completing_empty() {
        let (tx, mut rx) = mpsc::channel(32);
        let basket = Arc::new(RecordingBasket::default());
        let runner = MeshRunner::new(
            Arc::new(StubClaude::new(vec![
                StubClaude::tool_call("t1", "decompose_intent", serde_json::json!({
                    "sub_intents": [{"vertical": "pharmacy", "raw_text": "paracetamol", "constraints": {}}]
                })),
                StubClaude::text("split"),
            ])),
            Arc::new(InMemoryStore::default()),
            basket.clone(),
            Arc::new(NoopCatalog),
            MeshConfig::default(),
        );

        runner.run(TenantId::from_uuid(Uuid::new_v4()), Uuid::new_v4(), "paracetamol".into(), 14.5995, 120.9842, tx).await;

        let events = collect(&mut rx);
        assert!(events.iter().any(|e| matches!(e, MeshEvent::Failed { .. })));
        assert!(basket.created.lock().unwrap().is_empty(),
                "no basket should be created when nothing can be worked on");
    }
}

#[cfg(test)]
mod per_run_tool_box {
    use super::*;
    use logisticos_agent_runtime::testing::{InMemoryStore, StubClaude};
    use std::sync::Mutex;

    /// The runner needs a basket to construct; these tests never write one.
    struct NoBasket;
    #[async_trait::async_trait]
    impl crate::tools::MeshBasket for NoBasket {
        async fn create(&self, _: Uuid, _: Uuid) -> anyhow::Result<Uuid> { Ok(Uuid::new_v4()) }
        async fn write_delta(&self, _: Uuid, _: Uuid, _: Uuid, _: &str, _: &str,
                             _: Vec<ProposedLine>) -> anyhow::Result<()> { Ok(()) }
        async fn lines_awaiting_review(&self, _: Uuid, _: Uuid) -> anyhow::Result<usize> { Ok(0) }
        async fn record_conflicts(&self, _: Uuid, _: Uuid, _: &[crate::conflict::Conflict])
            -> anyhow::Result<()> { Ok(()) }
    }

    /// Records what the tool box actually asked the catalog for.
    #[derive(Default)]
    struct SpyCatalog {
        seen: Mutex<Vec<(Uuid, f64, f64)>>,
    }

    #[async_trait::async_trait]
    impl crate::tools::MeshCatalog for SpyCatalog {
        async fn search(&self, _: Uuid, _: Uuid, _: &str, _: &[String], _: i64)
            -> anyhow::Result<serde_json::Value> { Ok(serde_json::json!({ "items": [] })) }
        async fn vendors_near(&self, tenant: Uuid, _: &str, lat: f64, lng: f64, _: f64, _: i64)
            -> anyhow::Result<serde_json::Value> {
            self.seen.lock().unwrap().push((tenant, lat, lng));
            Ok(serde_json::json!({ "vendors": [] }))
        }
        async fn courier_supply(&self, _: Uuid, _: f64, _: f64, _: f64)
            -> anyhow::Result<serde_json::Value> { Ok(serde_json::json!({ "available": 1 })) }
        async fn resolve_facts(&self, _: Uuid, _: &[Uuid])
            -> anyhow::Result<Vec<crate::conflict::ItemFacts>> { Ok(vec![]) }
    }

    fn runner_with(catalog: Arc<SpyCatalog>) -> MeshRunner {
        MeshRunner::new(
            Arc::new(StubClaude::new(vec![StubClaude::text("ok")])),
            Arc::new(InMemoryStore::default()),
            Arc::new(NoBasket),
            catalog as Arc<dyn crate::tools::MeshCatalog>,
            MeshConfig::default(),
        )
    }

    /// The bug this replaced: the tool box was built once at startup, so every
    /// run — every customer, every tenant — searched from the configured
    /// default. Two runs must reach the catalog with their *own* coordinates.
    #[tokio::test]
    async fn each_run_searches_from_its_own_delivery_point() {
        let catalog = Arc::new(SpyCatalog::default());
        let runner = runner_with(catalog.clone());

        let manila = runner.tools_for(Uuid::new_v4(), 14.5995, 120.9842);
        let cebu   = runner.tools_for(Uuid::new_v4(), 10.3157, 123.8854);

        for t in [&manila, &cebu] {
            let _ = t.execute("find_vendors".to_string(),
                serde_json::json!({ "vertical": "restaurant" }),
                "t".into(), logisticos_agent_runtime::tools::ToolContext::default()).await;
        }

        let seen = catalog.seen.lock().unwrap().clone();
        assert_eq!(seen.len(), 2, "both runs must have searched, got {seen:?}");
        assert_eq!((seen[0].1, seen[0].2), (14.5995, 120.9842));
        assert_eq!((seen[1].1, seen[1].2), (10.3157, 123.8854),
                   "the second run must search from its own point, not the first's");
    }

    /// Tenant is bound per run for the same reason. A shared box would search
    /// one tenant's catalog on behalf of another.
    #[tokio::test]
    async fn each_run_carries_its_own_tenant() {
        let catalog = Arc::new(SpyCatalog::default());
        let runner = runner_with(catalog.clone());
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());

        for t in [runner.tools_for(a, 1.0, 2.0), runner.tools_for(b, 1.0, 2.0)] {
            let _ = t.execute("find_vendors".to_string(),
                serde_json::json!({ "vertical": "grocery" }),
                "t".into(), logisticos_agent_runtime::tools::ToolContext::default()).await;
        }

        let seen = catalog.seen.lock().unwrap().clone();
        assert_eq!(seen[0].0, a);
        assert_eq!(seen[1].0, b);
        assert_ne!(seen[0].0, seen[1].0);
    }
}
