import { createApiClient } from "./client";

// ── Types ──────────────────────────────────────────────────────────────────────
// Mirrors services/identity/src/domain/entities/api_key.rs.

export interface ApiKey {
  id: string | { 0: string };
  tenant_id: string | { 0: string };
  name: string;
  key_hash: string;            // client never needs this, but backend returns it
  key_prefix: string;          // safe-to-display "lsk_live_ab12..." form
  scopes: string[];
  is_active: boolean;
  expires_at?: string | null;
  last_used_at?: string | null;
  created_at: string;
}

export interface CreateApiKeyPayload {
  name: string;                 // 1-100 chars
  scopes: string[];
  /** If omitted → never expires. Clamped server-side. */
  expires_in_days?: number;
}

export interface CreateApiKeyResult {
  key_id: string;
  raw_key: string;              // returned ONCE; client must display + store
  key_prefix: string;
  scopes: string[];
  expires_at?: string | null;
}

// ── Helpers ────────────────────────────────────────────────────────────────────

export function apiKeyIdOf(k: ApiKey): string {
  const raw = k.id as unknown;
  if (typeof raw === "string") return raw;
  if (raw && typeof raw === "object" && "0" in raw) return String((raw as { 0: string })[0]);
  return "";
}

// ── Client ─────────────────────────────────────────────────────────────────────

export const apiKeysApi = {
  async list(): Promise<ApiKey[]> {
    const { data } = await createApiClient().get<{ data: ApiKey[] }>("/v1/api-keys");
    return data.data ?? [];
  },

  async create(payload: CreateApiKeyPayload): Promise<CreateApiKeyResult> {
    const { data } = await createApiClient().post<{ data: CreateApiKeyResult }>("/v1/api-keys", payload);
    return data.data;
  },

  async revoke(keyId: string): Promise<void> {
    await createApiClient().delete<void>(`/v1/api-keys/${keyId}`);
  },
};
