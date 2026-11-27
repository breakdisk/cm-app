/**
 * Server-side intent routing for the prompt box (`POST /v1/agents/classify`).
 *
 * The server reads the sentence and says whether it is a booking, a
 * whole-home move or a question about a running job, and what it states.
 * The on-device regex stays as the fallback: offline, on a plan without AI,
 * on a slow answer, and when the server is unsure. Either way the plan
 * screen shows the result for the customer to correct — nothing is booked
 * from a parse alone.
 */
import { getAiClient } from './ai';
import type { ParsedMove } from '../../screens/move/parsePrompt';

export type RoutedIntent = 'book' | 'home_move' | 'support';

export interface Classification {
  intent: RoutedIntent;
  confidence: number;
  extracted: {
    items: { name: string; qty: number }[];
    from: string;
    to: string;
    when: string;
    /** What a whole-home sentence stated about the property; null fields were not stated. */
    property?: {
      property_type?: 'apartment' | 'villa' | 'offices' | null;
      bedrooms?: number | null;
      desks?: number | null;
      pickup_floor?: number | null;
      pickup_has_lift?: boolean | null;
      dropoff_floor?: number | null;
      dropoff_has_lift?: boolean | null;
    };
  };
}

/** Below this, the server's reading is not trusted over the regex's. */
export const MIN_CONFIDENCE = 0.5;
export const CLASSIFY_TIMEOUT_MS = 5_000;

/** The server's answer, or null on any failure, refusal or timeout. */
export async function classifyPrompt(
  text: string,
  hasActiveJob: boolean,
  timeoutMs: number = CLASSIFY_TIMEOUT_MS,
): Promise<Classification | null> {
  try {
    const response = await getAiClient().post<{ data: Classification }>(
      '/v1/agents/classify',
      { text, has_active_job: hasActiveJob },
      { timeout: timeoutMs },
    );
    return response.data?.data ?? null;
  } catch {
    return null;
  }
}

function statesSomething(p: ParsedMove): boolean {
  return p.items.length > 0 || !!p.from || !!p.to || !!p.when;
}

export interface Routed {
  intent: RoutedIntent;
  parsed: ParsedMove;
  /** What the booking records about how it was read. */
  intake: { source: string; intent?: RoutedIntent; confidence?: number; extracted: ParsedMove };
}

/**
 * Which reading to plan from. The server's, when it answered with enough
 * confidence and stated something; otherwise the regex's. Support follows
 * the server only when a job is running (the server enforces the same).
 */
export function route(
  ai: Classification | null,
  regex: ParsedMove,
  regexIntent: RoutedIntent,
  mode: 'prompt' | 'voice',
): Routed {
  const trusted = ai && ai.confidence >= MIN_CONFIDENCE ? ai : null;
  if (trusted) {
    const parsed: ParsedMove = {
      items: trusted.extracted.items.map((i) => ({ name: i.name, qty: Math.max(1, Math.round(i.qty)) })),
      from: trusted.extracted.from,
      to: trusted.extracted.to,
      when: trusted.extracted.when,
    };
    const useAi = trusted.intent !== 'book' || statesSomething(parsed);
    if (useAi) {
      return {
        intent: trusted.intent,
        parsed: statesSomething(parsed) ? parsed : regex,
        intake: { source: `${mode}_ai`, intent: trusted.intent, confidence: trusted.confidence, extracted: parsed },
      };
    }
  }
  return {
    intent: regexIntent,
    parsed: regex,
    intake: { source: `${mode}_offline`, intent: regexIntent, extracted: regex },
  };
}
