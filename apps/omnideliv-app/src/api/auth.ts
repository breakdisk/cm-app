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

export interface Session {
  token: string;
  userId: string;
}

async function post(path: string, body: unknown): Promise<Response> {
  return fetch(`${AUTH_BASE}${path}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
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
      res.status === 429
        ? "Too many attempts. Wait a moment and try again."
        : "We couldn't send a code to that number.",
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
      res.status === 401 || res.status === 400
        ? "That code didn't work. Check it and try again."
        : "Something went wrong signing you in.",
    );
  }

  // Identity wraps successful bodies in `data`.
  const body = (await res.json()) as {
    data?: { access_token?: string; user?: { id?: string } };
  };
  const token = body.data?.access_token;
  if (!token) throw new Error("Signed in, but no session came back.");

  await SecureStore.setItemAsync(TOKEN_KEY, token);
  return { token, userId: body.data?.user?.id ?? "" };
}

export async function currentToken(): Promise<string | null> {
  return SecureStore.getItemAsync(TOKEN_KEY);
}

export async function signOut(): Promise<void> {
  await SecureStore.deleteItemAsync(TOKEN_KEY);
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
