import { createApiClient } from "./client";

// ── Types ──────────────────────────────────────────────────────────────────────
// Mirror of services/business-logic/src/domain/entities/rule.rs.
// RuleTrigger/RuleCondition/RuleAction are Rust enums serialized in
// externally-tagged form (serde default): either a bare string for unit
// variants, or a single-key object for variants with data.
//
//   DeliveryFailed              → "DeliveryFailed"
//   DeliveryAttempted{attempts} → {"DeliveryAttempted": {"attempts": 3}}
//
// Rather than model every variant as a discriminated union (the enum set is
// large and still evolving), we expose them as `string | Record<string,
// unknown>` and provide a `ruleLabel()` helper for display.

export type RuleTriggerValue = string | Record<string, unknown>;
export type RuleConditionValue = string | Record<string, unknown>;
export type RuleActionValue = string | Record<string, unknown>;

export interface AutomationRule {
  id: string;
  tenant_id: string;
  name: string;
  description: string;
  is_active: boolean;
  trigger: RuleTriggerValue;
  conditions: RuleConditionValue[];
  actions: RuleActionValue[];
  priority: number;
  created_at: string;
}

export interface ListRulesResponse {
  data: AutomationRule[];
  total: number;
  page: number;
  per_page: number;
  total_pages: number;
}

export interface RuleExecution {
  id: string;
  rule_id: string;
  tenant_id: string;
  event_type: string;
  matched: boolean;
  actions_executed: number;
  duration_ms: number;
  error?: string | null;
  occurred_at: string;
}

// ── Display helpers ────────────────────────────────────────────────────────────
// Convert an enum value into a human-readable label. Falls back to
// JSON.stringify for unknown variants.
export function enumLabel(v: string | Record<string, unknown>): string {
  if (typeof v === "string") return camelToTitle(v);
  const keys = Object.keys(v);
  if (keys.length === 1) {
    const k = keys[0];
    const inner = v[k] as Record<string, unknown> | undefined;
    if (inner && typeof inner === "object") {
      const args = Object.entries(inner).map(([ik, iv]) => `${ik}=${iv}`).join(", ");
      return args.length > 0 ? `${camelToTitle(k)} (${args})` : camelToTitle(k);
    }
    return camelToTitle(k);
  }
  return JSON.stringify(v);
}

function camelToTitle(s: string): string {
  return s.replace(/([A-Z])/g, " $1").trim();
}

// ── Client ─────────────────────────────────────────────────────────────────────

export const rulesApi = {
  async list(opts: { isActive?: boolean; page?: number; perPage?: number } = {}): Promise<ListRulesResponse> {
    const http = createApiClient();
    const { data } = await http.get<ListRulesResponse>("/v1/rules", {
      params: {
        is_active: opts.isActive,
        page: opts.page ?? 1,
        per_page: opts.perPage ?? 50,
      },
    });
    return data;
  },

  async get(id: string): Promise<AutomationRule> {
    const http = createApiClient();
    const { data } = await http.get<{ data: AutomationRule }>(`/v1/rules/${id}`);
    return data.data;
  },

  /** PATCH /v1/rules/:id/toggle — flip is_active. Returns the updated rule. */
  async toggle(id: string): Promise<AutomationRule> {
    const http = createApiClient();
    const { data } = await http.patch<{ data: AutomationRule }>(`/v1/rules/${id}/toggle`);
    return data.data;
  },

  async delete(id: string): Promise<void> {
    const http = createApiClient();
    await http.delete<void>(`/v1/rules/${id}`);
  },

  /** POST /v1/rules/reload — force in-memory engine reload from the DB. */
  async reload(): Promise<{ rules_loaded: number }> {
    const http = createApiClient();
    const { data } = await http.post<{ rules_loaded: number }>("/v1/rules/reload");
    return data;
  },

  async executions(ruleId: string): Promise<RuleExecution[]> {
    const http = createApiClient();
    const { data } = await http.get<{ data: RuleExecution[] }>(`/v1/rules/${ruleId}/executions`);
    return data.data ?? [];
  },
};
