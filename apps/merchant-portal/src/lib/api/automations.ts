import { createApiClient } from "./client";

// ── Types (mirror services/business-logic/src/domain/entities/rule.rs) ─────────
// Rust serde adjacently-tagged enums serialize as { "<VariantName>": { ...fields } }
// for enum variants with fields, or "<VariantName>" for unit variants.

export type RuleTrigger =
  | "DeliveryFailed"
  | "DeliveryCompleted"
  | "ShipmentCreated"
  | "AssignNewDriver"
  | "RunAiDispatch"
  | { DeliveryAttempted: { attempts: number } }
  | { ShipmentDelayed: { minutes_late: number } }
  | { CustomerInactive: { days: number } }
  | { DriverIdleExceeded: { minutes: number } }
  | { HubCapacityExceeded: { threshold_pct: number } }
  | { PaymentOverdue: { days: number } }
  | { SlaBreach: { sla_type: string } }
  | { CampaignTriggered: { campaign_id: string } };

export type RuleCondition =
  | { ServiceType: { equals: string } }
  | { ShipmentValue: { greater_than: number } }
  | { Zone: { in_zones: string[] } }
  | { CustomerSegment: { segment_id: string } }
  | { TimeOfDay: { hour_from: number; hour_to: number } }
  | { DayOfWeek: { days: string[] } }
  | { AttemptCount: { lte: number } }
  | { MerchantTier: { tiers: string[] } }
  | { WeatherCondition: { condition: string } }
  | { Custom: { expression: string } };

export type RuleAction =
  | { RescheduleDelivery: { delay_hours: number } }
  | { NotifyCustomer: { channel: string; template_id: string } }
  | { NotifyMerchant: { template_id: string } }
  | { AlertDispatcher: { message: string; priority: string } }
  | "AssignNewDriver"
  | { UpdateShipmentStatus: { status: string } }
  | { ApplyDiscount: { percentage: number } }
  | { TriggerCampaign: { campaign_id: string } }
  | { CreateRefund: { reason: string } }
  | { EscalateToSupport: { tier: number } }
  | "RunAiDispatch"
  | { WebhookCall: { url: string; method: string } }
  | { LogAuditEvent: { event_type: string } };

export interface AutomationRule {
  id: string;
  tenant_id: string;
  name: string;
  description: string;
  is_active: boolean;
  trigger: RuleTrigger;
  conditions: RuleCondition[];
  actions: RuleAction[];
  priority: number;
  created_at: string;
}

export interface RuleExecution {
  id: string;
  rule_id: string;
  tenant_id: string;
  event_topic: string;
  shipment_id: string | null;
  conditions_met: boolean;
  actions_taken: string[];
  outcome: "success" | "error";
  error_message: string | null;
  fired_at: string;
}

export interface ListRulesResponse {
  data: AutomationRule[];
  total: number;
  page: number;
  per_page: number;
  total_pages: number;
}

export interface ListExecutionsResponse {
  data: RuleExecution[];
  next_cursor: string | null;
}

export interface CreateRulePayload {
  name: string;
  description?: string;
  trigger: RuleTrigger;
  conditions?: RuleCondition[];
  actions: RuleAction[];
  priority?: number;
}

export type UpdateRulePayload = CreateRulePayload;

export interface ValidateExpressionResponse {
  valid: boolean;
  error?: string;
}

// ── Helpers — human-readable labels ─────────────────────────────────────────────

export function triggerLabel(trigger: RuleTrigger): string {
  if (trigger === "DeliveryFailed") return "Delivery Failed";
  if (trigger === "DeliveryCompleted") return "Delivery Completed";
  if (trigger === "ShipmentCreated") return "Shipment Created";
  if (typeof trigger === "object") {
    if ("DeliveryAttempted" in trigger) return `Delivery Attempted (≤${trigger.DeliveryAttempted.attempts}×)`;
    if ("ShipmentDelayed" in trigger) return `Shipment Delayed >  ${trigger.ShipmentDelayed.minutes_late}min`;
    if ("CustomerInactive" in trigger) return `Customer Inactive ${trigger.CustomerInactive.days}d`;
    if ("PaymentOverdue" in trigger) return `Payment Overdue ${trigger.PaymentOverdue.days}d`;
    if ("SlaBreach" in trigger) return `SLA Breach: ${trigger.SlaBreach.sla_type}`;
    if ("CampaignTriggered" in trigger) return "Campaign Triggered";
  }
  return String(trigger);
}

export function actionLabel(action: RuleAction): string {
  if (action === "AssignNewDriver") return "Assign New Driver";
  if (action === "RunAiDispatch") return "Run AI Dispatch";
  if (typeof action === "object") {
    if ("RescheduleDelivery" in action) return `Reschedule +${action.RescheduleDelivery.delay_hours}h`;
    if ("NotifyCustomer" in action) return `Notify Customer (${action.NotifyCustomer.channel})`;
    if ("NotifyMerchant" in action) return "Notify Merchant";
    if ("AlertDispatcher" in action) return `Alert Dispatcher (${action.AlertDispatcher.priority})`;
    if ("TriggerCampaign" in action) return "Trigger Campaign";
    if ("EscalateToSupport" in action) return `Escalate to Support (Tier ${action.EscalateToSupport.tier})`;
    if ("LogAuditEvent" in action) return `Log: ${action.LogAuditEvent.event_type}`;
    if ("UpdateShipmentStatus" in action) return `Update Status → ${action.UpdateShipmentStatus.status}`;
    if ("WebhookCall" in action) return `Webhook: ${action.WebhookCall.method} ${action.WebhookCall.url}`;
  }
  return JSON.stringify(action);
}

export function conditionLabel(condition: RuleCondition): string {
  if (typeof condition === "object") {
    if ("CustomerSegment" in condition) return "Customer Segment";
    if ("ServiceType" in condition) return `Service: ${condition.ServiceType.equals}`;
    if ("ShipmentValue" in condition) return `Value > ${condition.ShipmentValue.greater_than}`;
    if ("AttemptCount" in condition) return `Attempts ≤ ${condition.AttemptCount.lte}`;
    if ("Zone" in condition) {
      const zones = condition.Zone.in_zones;
      return `Zone: ${zones.slice(0, 2).join(", ")}${zones.length > 2 ? "…" : ""}`;
    }
    if ("TimeOfDay" in condition) return `${condition.TimeOfDay.hour_from}h–${condition.TimeOfDay.hour_to}h`;
    if ("DayOfWeek" in condition) return condition.DayOfWeek.days.slice(0, 3).join(", ");
    if ("MerchantTier" in condition) return `Tier: ${condition.MerchantTier.tiers.join(", ")}`;
    if ("WeatherCondition" in condition) return `Weather: ${condition.WeatherCondition.condition}`;
    if ("Custom" in condition) return "Custom";
  }
  return JSON.stringify(condition).slice(0, 30);
}

// ── Client ───────────────────────────────────────────────────────────────────────

export function createAutomationsApi() {
  const http = createApiClient();

  return {
    async list(page = 1, perPage = 20, isActive?: boolean): Promise<ListRulesResponse> {
      const { data } = await http.get<ListRulesResponse>("/v1/rules", {
        params: { page, per_page: perPage, ...(isActive !== undefined ? { is_active: isActive } : {}) },
      });
      return data;
    },

    async get(id: string): Promise<AutomationRule> {
      const { data } = await http.get<{ data: AutomationRule }>(`/v1/rules/${id}`);
      return data.data;
    },

    async create(payload: CreateRulePayload): Promise<AutomationRule> {
      const { data } = await http.post<{ data: AutomationRule }>("/v1/rules", payload);
      return data.data;
    },

    async update(id: string, payload: UpdateRulePayload): Promise<AutomationRule> {
      const { data } = await http.put<{ data: AutomationRule }>(`/v1/rules/${id}`, payload);
      return data.data;
    },

    async remove(id: string): Promise<void> {
      await http.delete(`/v1/rules/${id}`);
    },

    async toggle(id: string, isActive: boolean): Promise<AutomationRule> {
      const { data } = await http.patch<{ data: AutomationRule }>(`/v1/rules/${id}/toggle`, { is_active: isActive });
      return data.data;
    },

    async executions(id: string, limit = 20, cursor?: string): Promise<ListExecutionsResponse> {
      const { data } = await http.get<ListExecutionsResponse>(`/v1/rules/${id}/executions`, {
        params: { limit, ...(cursor ? { cursor } : {}) },
      });
      return data;
    },

    async validateExpression(expression: string): Promise<ValidateExpressionResponse> {
      const { data } = await http.post<ValidateExpressionResponse>("/v1/rules/validate-expression", { expression });
      return data;
    },
  };
}

// Singleton — avoids creating a new axios instance on every render.
let _api: ReturnType<typeof createAutomationsApi> | null = null;
export function getAutomationsApi() {
  if (!_api) _api = createAutomationsApi();
  return _api;
}
