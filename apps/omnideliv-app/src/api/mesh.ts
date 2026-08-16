/** Mirrors omnideliv-mesh's MeshEvent. Keep the two in sync by hand. */
export type MeshEvent =
  | { event: "intent_parsed"; sub_intent_count: number }
  | { event: "specialist_started"; sub_intent_id: string; role: string; vertical: string; label: string }
  | { event: "specialist_progress"; sub_intent_id: string; note: string }
  | { event: "specialist_finished"; sub_intent_id: string; lines_added: number; degraded: boolean; note: string | null }
  | { event: "constraint_detected"; description: string }
  | { event: "route_planned"; stops: number; flat_fee_cents: number; total_minutes: number }
  | { event: "completed"; basket_id: string; needs_review: number }
  | { event: "failed"; reason: string };

/** Terminal events end the stream. Mirrors MeshEvent::is_terminal. */
export function isTerminal(e: MeshEvent): boolean {
  return e.event === "completed" || e.event === "failed";
}
