/**
 * Phone OTP sign-in.
 *
 * The same rail the platform's other consumer app uses
 * (`POST /v1/auth/otp/send` → `/v1/auth/otp/verify`), deliberately rather than
 * a third pattern: identity already auto-registers on first verify, so there is
 * no separate signup to build, and a second bespoke auth flow is the copying
 * ADR-0009 rule 4 exists to prevent.
 *
 * Phone rather than email is also the right identifier for delivery. The
 * courier may need to call, and it is the contact detail order notifications
 * currently lack — they are push-only because an OmniDeliv order carries no
 * phone number. Signing in with one is what eventually unblocks SMS/WhatsApp.
 */
import * as SecureStore from "expo-secure-store";

/** Identity lives behind the gateway, not on the omnideliv service. */
const AUTH_BASE =
  process.env.EXPO_PUBLIC_GATEWAY_API ?? "http://localhost:8000";

/** Which tenant this build belongs to. */
const TENANT_SLUG = process.env.EXPO_PUBLIC_TENANT_SLUG ?? "demo";

export const TOKEN_KEY = "auth_token";
/**
 * The access token lives 60 minutes. Without persisting this alongside it the
 * app simply stopped working an hour after sign-in: every request came back
 * `{"error":"Invalid or expired token"}` and there was no way out but a
 * reinstall, because nothing refreshed and nothing signed you out.
 */
export const REFRESH_KEY = "refresh_token";

export interface Session {
  token: string;
  userId: string;
}

/**
 * Whether we hold a token, answerable *synchronously*.
 *
 * The root layout gate needs this the instant navigation happens, and
 * SecureStore is async. It used to solve that by reading the token once on
 * mount into React state — which meant that after a successful sign-in the gate
 * still believed you were signed out, bounced you from "/" straight back to
 * "/sign-in", and made a correct OTP look rejected. It only came right on the
 * next cold start, when the mount effect ran again.
 *
 * Same shape as `deliveryPoint.ts`: a module-level cache, primed once, kept
 * current by every writer. There is deliberately no React state involved — a
 * gate that can hold a stale answer is the bug.
 */
let cachedToken: string | null = null;

/** Populate the cache. Call once at startup, before the gate reads it. */
export async function loadSession(): Promise<void> {
  try {
    cachedToken = await SecureStore.getItemAsync(TOKEN_KEY);
  } catch {
    // An unreadable store is the same as being signed out: ask again rather
    // than stranding someone in a session we cannot prove.
    cachedToken = null;
  }
}

export function isSignedIn(): boolean {
  return cachedToken !== null;
}

async function post(path: string, body: unknown): Promise<Response> {
  return fetch(`${AUTH_BASE}${path}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
}


/**
 * A message that says what actually happened.
 *
 * These used to collapse every failure into one of three fixed strings, so a
 * report of "cannot log in" carried no status, no server message, and no way
 * to tell a rejected code from an unreachable host. The friendly sentence
 * stays first; the detail is appended so a screenshot is diagnosable.
 */
async function describe(res: Response, friendly: string): Promise<string> {
  const raw = await res.text().catch(() => "");
  let detail = raw.trim();
  try {
    const parsed = JSON.parse(detail) as { error?: { message?: string } | string };
    const msg = typeof parsed.error === "string" ? parsed.error : parsed.error?.message;
    if (msg) detail = msg;
  } catch {
    // Not JSON — a proxy or gateway page. Keep it short.
  }
  const suffix = detail ? ` (${res.status}: ${detail.slice(0, 120)})` : ` (${res.status})`;
  return friendly + suffix;
}

/**
 * Ask for a code. Resolves on success; throws with a readable message so the
 * screen can show why rather than a status number.
 */
export async function requestOtp(phone: string): Promise<void> {
  const res = await post("/v1/auth/otp/send", {
    phone_number: phone,
    tenant_slug: TENANT_SLUG,
    // Auto-registration role. "customer", not the endpoint's "driver" default —
    // getting this wrong would create delivery customers as drivers.
    role: "customer",
  });
  if (!res.ok) {
    throw new Error(
      await describe(
        res,
        res.status === 429
          ? "Too many attempts. Wait a moment and try again."
          : "We couldn't send a code to that number.",
      ),
    );
  }
}

/**
 * Exchange the code for a session and store it.
 *
 * Auto-registers on first use, so "sign in" and "sign up" are the same action —
 * which is why this app has no separate registration screen.
 */
export async function verifyOtp(phone: string, code: string): Promise<Session> {
  const res = await post("/v1/auth/otp/verify", {
    phone_number: phone,
    otp_code: code,
    tenant_slug: TENANT_SLUG,
    role: "customer",
  });

  if (!res.ok) {
    throw new Error(
      await describe(
        res,
        res.status === 401 || res.status === 400
          ? "That code didn't work. Check it and try again."
          : "Something went wrong signing you in.",
      ),
    );
  }

  // Identity wraps successful bodies in `data`.
  const body = (await res.json()) as {
    data?: { access_token?: string; refresh_token?: string; user?: { id?: string } };
  };
  const token = body.data?.access_token;
  if (!token) throw new Error("Signed in, but no session came back.");

  cachedToken = token;
  await SecureStore.setItemAsync(TOKEN_KEY, token);
  // The server returns this and the app used to drop it on the floor.
  if (body.data?.refresh_token) {
    await SecureStore.setItemAsync(REFRESH_KEY, body.data.refresh_token);
  }
  return { token, userId: body.data?.user?.id ?? "" };
}

export async function currentToken(): Promise<string | null> {
  // Reads through and refreshes the cache, so the sync and async answers can
  // never drift apart.
  cachedToken = await SecureStore.getItemAsync(TOKEN_KEY);
  return cachedToken;
}

export async function signOut(): Promise<void> {
  cachedToken = null;
  await SecureStore.deleteItemAsync(TOKEN_KEY);
  await SecureStore.deleteItemAsync(REFRESH_KEY);
}

/**
 * Trade the refresh token for a new session.
 *
 * Returns the new access token, or `null` when there is nothing to refresh
 * with or the server refuses — in which case the caller should sign out and
 * send the person back to the phone screen rather than retry forever.
 *
 * The refresh token is rotated on every use, so the new one is stored too;
 * keeping the old one would make the *next* refresh fail.
 */
export async function refreshSession(): Promise<string | null> {
  const refresh = await SecureStore.getItemAsync(REFRESH_KEY);
  if (!refresh) return null;

  let res: Response;
  try {
    res = await post("/v1/auth/refresh", { refresh_token: refresh });
  } catch {
    // Offline. Not a dead session — leave the tokens alone so it can retry.
    return null;
  }
  if (!res.ok) {
    await signOut();
    return null;
  }

  const body = (await res.json().catch(() => ({}))) as {
    data?: { access_token?: string; refresh_token?: string };
  };
  const token = body.data?.access_token;
  if (!token) {
    await signOut();
    return null;
  }

  cachedToken = token;
  await SecureStore.setItemAsync(TOKEN_KEY, token);
  if (body.data?.refresh_token) {
    await SecureStore.setItemAsync(REFRESH_KEY, body.data.refresh_token);
  }
  return token;
}

/**
 * E.164-ish normalisation for a Philippine number.
 *
 * Local habit is to type `09171234567`; identity wants `+639171234567`. Doing
 * this in one place means the number stored against the account matches the one
 * a courier would dial, rather than depending on how the customer typed it.
 */
export function normalisePhone(input: string): string {
  const digits = input.replace(/[^\d+]/g, "");
  if (digits.startsWith("+")) return digits;
  if (digits.startsWith("0")) return `+63${digits.slice(1)}`;
  if (digits.startsWith("63")) return `+${digits}`;
  return `+63${digits}`;
}
