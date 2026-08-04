/**
 * AI Support chat — wraps the AI Intelligence Layer's customer chat endpoint
 * (`POST /v1/agents/chat`, routed through the API gateway to ai-layer).
 *
 * The endpoint runs a Claude-backed agent restricted to the CustomerSupport
 * tool allowlist (look up a shipment, reschedule a delivery, hand over to a
 * human). Pass the `session_id` from the previous reply back on the next turn
 * to keep the conversation's context.
 *
 * Access is gated on the tenant's AI tier. A 403 here is expected on non-AI
 * plans and is not an error state — SupportScreen falls back to its offline
 * FAQ answers rather than showing a failure.
 */
import type { AxiosInstance } from 'axios';
import * as SecureStore from 'expo-secure-store';
import { createApiClient, ApiError } from './client';

let cachedAiClient: AxiosInstance | null = null;

function getAiClient(): AxiosInstance {
  if (!cachedAiClient) {
    cachedAiClient = createApiClient(
      process.env.EXPO_PUBLIC_AI_URL || process.env.EXPO_PUBLIC_API_URL || 'http://localhost:8016'
    );
  }
  return cachedAiClient;
}

/** Shipment index sent with each turn so the agent can resolve "my Cebu parcel". */
export interface ChatShipmentContext {
  id?: string;
  awb?: string;
  status?: string;
}

export interface ChatRequest {
  /** Omit on the first message; echo the previous reply's sessionId after that. */
  sessionId?: string;
  message: string;
  shipments?: ChatShipmentContext[];
}

export type ChatStatus = 'running' | 'completed' | 'failed' | 'human_escalated';

export interface ChatReply {
  session_id: string;
  reply: string;
  escalated: boolean;
  status: ChatStatus;
}

/** State of a conversation, used to pick up an operator's reply after the fact. */
export interface ChatState {
  session_id: string;
  status: ChatStatus;
  /** Still sitting in the ops human-review queue. */
  escalated: boolean;
  /** A human operator has since answered and closed it. */
  resolved_by_human: boolean;
  /** Latest assistant turn — the operator's note once resolved. */
  latest_reply: string | null;
}

/** Thrown when the tenant's plan does not include AI support chat. */
export class AiUnavailableError extends Error {
  constructor(message = 'AI support is not enabled on this account') {
    super(message);
    this.name = 'AiUnavailableError';
  }
}

/**
 * The escalated conversation id is kept outside component state so a customer
 * can close the app, get the "your support request has an answer" push days
 * later, and still land back in the same thread.
 */
const SESSION_KEY = 'support_chat_session';

export async function getStoredChatSession(): Promise<string | null> {
  try {
    return await SecureStore.getItemAsync(SESSION_KEY);
  } catch {
    return null;
  }
}

export async function storeChatSession(sessionId: string): Promise<void> {
  try {
    await SecureStore.setItemAsync(SESSION_KEY, sessionId);
  } catch {
    /* best-effort — losing this only costs conversation continuity */
  }
}

export async function clearChatSession(): Promise<void> {
  try {
    await SecureStore.deleteItemAsync(SESSION_KEY);
  } catch {
    /* best-effort */
  }
}

export const aiApi = {
  /** Current state of a conversation — how an operator's later reply is picked up. */
  async getChat(sessionId: string): Promise<ChatState> {
    const response = await getAiClient().get<ChatState>(`/v1/agents/chat/${sessionId}`);
    return response.data;
  },

  async chat({ sessionId, message, shipments = [] }: ChatRequest): Promise<ChatReply> {
    try {
      const response = await getAiClient().post<ChatReply>('/v1/agents/chat', {
        session_id: sessionId,
        message,
        shipments,
      });
      return response.data;
    } catch (e: unknown) {
      if (e instanceof ApiError && e.status === 403) {
        throw new AiUnavailableError(e.message);
      }
      throw e;
    }
  },
};
