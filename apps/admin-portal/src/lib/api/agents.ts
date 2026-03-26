import { createApiClient, ApiResponse, PaginatedApiResponse } from "./client";

export interface AgentSession {
  id: string;
  agent_type: "dispatch" | "recovery" | "merchant_support" | "reconciliation" | "on_demand";
  status: "running" | "completed" | "escalated" | "failed";
  outcome?: string;
  escalation_reason?: string;
  confidence_score: number;
  actions_taken: number;
  started_at: string;
  completed_at?: string;
}

export interface RunAgentRequest {
  prompt: string;
  context?: Record<string, unknown>;
}

export interface RunAgentResponse {
  session_id: string;
  status: string;
  outcome: string;
  confidence: number;
  actions_taken: number;
  escalated: boolean;
  escalation_reason?: string;
}

export const agentsApi = {
  /** List all agent sessions (ops visibility) */
  listSessions: (
    params: {
      status?: string;
      agent_type?: string;
      limit?: number;
      offset?: number;
    },
    token: string
  ) =>
    createApiClient(token)
      .get<{ sessions: AgentSession[]; count: number }>(
        "/v1/agents/sessions",
        { params }
      )
      .then((r) => r.data),

  /** Get sessions awaiting human review */
  getEscalated: (token: string) =>
    createApiClient(token)
      .get<{ escalated: AgentSession[]; count: number }>(
        "/v1/agents/sessions/escalated"
      )
      .then((r) => r.data),

  /** Get full session details including message history */
  getSession: (sessionId: string, token: string) =>
    createApiClient(token)
      .get<AgentSession>(`/v1/agents/sessions/${sessionId}`)
      .then((r) => r.data),

  /** Trigger an on-demand agent task */
  runAgent: (payload: RunAgentRequest, token: string) =>
    createApiClient(token)
      .post<RunAgentResponse>("/v1/agents/run", payload)
      .then((r) => r.data),

  /** Human resolves an escalated agent session */
  resolveEscalation: (
    sessionId: string,
    resolutionNotes: string,
    token: string
  ) =>
    createApiClient(token)
      .post<{ resolved: boolean; session_id: string }>(
        `/v1/agents/sessions/${sessionId}/resolve`,
        { resolution_notes: resolutionNotes }
      )
      .then((r) => r.data),
};
