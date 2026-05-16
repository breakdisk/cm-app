import { authFetch } from "@/lib/auth/auth-fetch";

const ENGAGEMENT_URL =
  process.env.NEXT_PUBLIC_ENGAGEMENT_URL ?? "http://localhost:8003";

// ── Types ──────────────────────────────────────────────────────────────────────

export interface ChannelView {
  enabled: boolean;
  configured: boolean;
  /** Last 4 chars of account SID, e.g. "****5678" */
  account_sid_hint?: string;
  from_number?: string;
  from_address?: string;
  from_name?: string;
}

export interface ChannelConfigView {
  whatsapp: ChannelView;
  sms: ChannelView;
  email: ChannelView;
  push: ChannelView;
}

export interface ChannelHealth {
  enabled: boolean;
  configured: boolean;
  provider: string;
  provider_ready: boolean;
}

export interface ChannelsHealthResponse {
  channels: {
    whatsapp: ChannelHealth;
    sms: ChannelHealth;
    email: ChannelHealth;
    push: ChannelHealth;
  };
}

export interface WhatsAppConfigUpdate {
  enabled?: boolean;
  account_sid?: string;
  /** Leave empty to keep existing token unchanged. */
  auth_token?: string;
  from_number?: string;
}

export interface SmsConfigUpdate {
  enabled?: boolean;
  account_sid?: string;
  auth_token?: string;
  from_number?: string;
}

export interface EmailConfigUpdate {
  enabled?: boolean;
  from_address?: string;
  from_name?: string;
}

export interface PushConfigUpdate {
  enabled?: boolean;
}

export interface UpdateChannelConfigRequest {
  whatsapp?: WhatsAppConfigUpdate;
  sms?: SmsConfigUpdate;
  email?: EmailConfigUpdate;
  push?: PushConfigUpdate;
}

// ── Client ─────────────────────────────────────────────────────────────────────

async function engagementFetch(path: string, init?: RequestInit): Promise<Response> {
  return authFetch(`${ENGAGEMENT_URL}${path}`, init);
}

export const channelsApi = {
  async getConfig(): Promise<ChannelConfigView> {
    const res = await engagementFetch("/v1/channel-configs");
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    const json = await res.json() as { config: ChannelConfigView };
    return json.config;
  },

  async updateConfig(req: UpdateChannelConfigRequest): Promise<ChannelConfigView> {
    const res = await engagementFetch("/v1/channel-configs", {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(req),
    });
    if (!res.ok) {
      const body = await res.text();
      throw new Error(body || `HTTP ${res.status}`);
    }
    const json = await res.json() as { config: ChannelConfigView };
    return json.config;
  },

  async getHealth(): Promise<ChannelsHealthResponse> {
    const res = await engagementFetch("/v1/channels/health");
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    return res.json() as Promise<ChannelsHealthResponse>;
  },
};
