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
use crate::transition::{MeshTransition, ProposedLine, SubIntentSpec};

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
            fanout_deadline: Duration::from_secs(8),
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
    pub degraded_count: usize,
    /// Every worker degraded — the mesh produced nothing usable and the client
    /// should fall back to deterministic browse.
    pub total_failure:  bool,
}

/// Phase 3. The single writer: merges results, counts degradations, and decides
/// whether the run produced anything usable.
pub fn reconcile_results(results: Vec<SpecialistResult>) -> MeshOutcome {
    let degraded_count = results.iter().filter(|r| r.degraded).count();
    let total_failure = !results.is_empty() && degraded_count == results.len();

    let lines = results
        .into_iter()
        .filter(|r| !r.degraded)
        .map(|r| (r.sub_intent_id, r.lines))
        .collect();

    MeshOutcome { lines, degraded_count, total_failure }
}

pub struct MeshRunner {
    claude: Arc<dyn ClaudeApi>,
    tools:  Arc<dyn ToolBox>,
    store:  Arc<dyn SessionStore>,
    config: MeshConfig,
}

impl MeshRunner {
    pub fn new(
        claude: Arc<dyn ClaudeApi>,
        tools: Arc<dyn ToolBox>,
        store: Arc<dyn SessionStore>,
        config: MeshConfig,
    ) -> Self {
        Self { claude, tools, store, config }
    }

    /// Phase 1. Returns the parent session id alongside the split, so every
    /// specialist can be linked to the run that spawned it.
    pub async fn parse(
        &self,
        tenant_id: TenantId,
        utterance: String,
    ) -> anyhow::Result<(Uuid, Vec<SubIntentSpec>)> {
        let runner = AgentRunner::new(self.claude.clone(), self.tools.clone(), self.store.clone());

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

    /// Phase 2. One concurrent task per worker, joined under a shared deadline.
    ///
    /// A worker still running when the deadline passes is abandoned and its
    /// vertical degrades — the order proceeds with what the others returned.
    /// The deadline is shared rather than per-worker so the customer's total
    /// wait is bounded regardless of fan-out width: five specialists must not
    /// mean five times the wait.
    pub async fn fan_out(
        &self,
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
            let tools  = self.tools.clone();
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

    struct NoopCatalog;

    #[async_trait::async_trait]
    impl crate::tools::MeshCatalog for NoopCatalog {
        async fn search(&self, _: Uuid, _: Uuid, _: &str, _: &[String], _: i64)
            -> anyhow::Result<serde_json::Value> { Ok(serde_json::json!({ "items": [] })) }
        async fn vendors_near(&self, _: Uuid, _: &str, _: f64, _: f64, _: f64, _: i64)
            -> anyhow::Result<serde_json::Value> { Ok(serde_json::json!({ "vendors": [] })) }
        async fn courier_supply(&self, _: Uuid, _: f64, _: f64, _: f64)
            -> anyhow::Result<serde_json::Value> { Ok(serde_json::json!({ "available": 6 })) }
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
        ]);

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
        ]);

        assert!(outcome.total_failure);
    }

    /// An empty-but-successful result is not a failure: the specialist looked
    /// and honestly found nothing.
    #[tokio::test]
    async fn an_honest_empty_result_is_not_a_failure() {
        let outcome = reconcile_results(vec![
            SpecialistResult { sub_intent_id: Uuid::new_v4(), lines: vec![], degraded: false,
                               note: Some("no eggs anywhere nearby".into()) },
        ]);

        assert!(!outcome.total_failure);
        assert_eq!(outcome.degraded_count, 0);
    }

    #[tokio::test]
    async fn the_parent_session_is_recorded_before_any_specialist_runs() {
        let store = Arc::new(InMemoryStore::default());
        let runner = MeshRunner::new(
            Arc::new(StubClaude::new(vec![StubClaude::text("ok")])),
            Arc::new(crate::tools::MeshToolBox::new(
                Arc::new(NoopCatalog), Uuid::new_v4(), 14.5995, 120.9842)),
            store.clone(),
            MeshConfig::default(),
        );

        let _ = runner
            .parse(TenantId::from_uuid(Uuid::new_v4()), "dinner and milk".into())
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
}
