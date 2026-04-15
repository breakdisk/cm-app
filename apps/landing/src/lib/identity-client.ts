/**
 * Server-only client for the LogisticOS identity service.
 *
 * Used by the landing app's auth routes to convert a Firebase-authenticated
 * caller into a LogisticOS JWT pair. The exchange endpoint lives behind the
 * identity service's `/internal` router and requires `X-Internal-Secret`;
 * `/v1/auth/refresh` is public. Neither call ever leaves the backend mesh —
 * the browser only ever sees the cookies we write in response.
 */

const IDENTITY_URL =
  process.env.IDENTITY_SERVICE_URL ??
  process.env.IDENTITY_URL ??
  "http://localhost:8001";

interface ExchangeBody {
  firebase_uid: string;
  email: string;
  email_verified: boolean;
  role: string;
  display_name?: string;
  partner_slug?: string;
  partner_sig?: string;
}

export interface ExchangeResult {
  access_token: string;
  refresh_token: string;
  expires_in: number;
  token_type: string;
  user: {
    id: string;
    tenant_id: string;
    tenant_slug: string;
    email: string;
    roles: string[];
    onboarding_required: boolean;
  };
}

export interface RefreshResult {
  access_token: string;
  refresh_token: string;
  expires_in: number;
  token_type: string;
}

export class IdentityError extends Error {
  constructor(public status: number, public code: string, message: string) {
    super(message);
    this.name = "IdentityError";
  }
}

export async function exchangeFirebaseToken(body: ExchangeBody): Promise<ExchangeResult> {
  const secret = process.env.LOGISTICOS_INTERNAL_SECRET;
  if (!secret) {
    throw new IdentityError(
      500,
      "INTERNAL_SECRET_MISSING",
      "LOGISTICOS_INTERNAL_SECRET is not set",
    );
  }

  const res = await fetch(`${IDENTITY_URL}/v1/internal/auth/exchange-firebase`, {
    method:  "POST",
    headers: {
      "Content-Type":     "application/json",
      "X-Internal-Secret": secret,
    },
    body: JSON.stringify(body),
    // Never let Next cache a mutation.
    cache: "no-store",
  });

  if (!res.ok) {
    const payload = await safeJson(res);
    throw new IdentityError(
      res.status,
      payload?.error?.code ?? "EXCHANGE_FAILED",
      payload?.error?.message ?? `Identity exchange returned ${res.status}`,
    );
  }

  return (await res.json()) as ExchangeResult;
}

export interface FinalizeTenantBody {
  business_name: string;
  currency: string;
  region: string;
}

export interface FinalizeTenantResult {
  tenant_id: string;
  slug: string;
  name: string;
  status: string;
}

export async function finalizeTenantSelf(
  accessToken: string,
  body: FinalizeTenantBody,
): Promise<FinalizeTenantResult> {
  const res = await fetch(`${IDENTITY_URL}/v1/tenants/me/finalize`, {
    method:  "POST",
    headers: {
      "Content-Type":        "application/json",
      "Authorization":       `Bearer ${accessToken}`,
      "X-LogisticOS-Client": "service",
    },
    body:  JSON.stringify(body),
    cache: "no-store",
  });

  if (!res.ok) {
    const payload = await safeJson(res);
    throw new IdentityError(
      res.status,
      payload?.error?.code ?? "FINALIZE_FAILED",
      payload?.error?.message ?? `Identity finalize returned ${res.status}`,
    );
  }

  return ((await res.json()) as { data: FinalizeTenantResult }).data;
}

export async function refreshLosToken(refreshToken: string): Promise<RefreshResult> {
  const res = await fetch(`${IDENTITY_URL}/v1/auth/refresh`, {
    method:  "POST",
    headers: { "Content-Type": "application/json" },
    body:    JSON.stringify({ refresh_token: refreshToken }),
    cache:   "no-store",
  });

  if (!res.ok) {
    const payload = await safeJson(res);
    throw new IdentityError(
      res.status,
      payload?.error?.code ?? "REFRESH_FAILED",
      payload?.error?.message ?? `Identity refresh returned ${res.status}`,
    );
  }

  return (await res.json()) as RefreshResult;
}

async function safeJson(res: Response): Promise<any> {
  try {
    return await res.json();
  } catch {
    return null;
  }
}
