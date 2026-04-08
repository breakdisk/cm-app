import { initializeApp, getApps, cert, type App } from "firebase-admin/app";
import { getAuth } from "firebase-admin/auth";

function getAdminApp(): App {
  if (getApps().length) return getApps()[0];
  const serviceAccount = JSON.parse(
    Buffer.from(process.env.FIREBASE_SERVICE_ACCOUNT_JSON!, "base64").toString("utf8")
  );
  return initializeApp({ credential: cert(serviceAccount) });
}

export interface SessionPayload {
  uid?:    string;
  email?:  string | undefined;
  role?:   string | undefined;
  expired: boolean;
}

/**
 * Verifies a Firebase ID token from the __session cookie.
 * Returns SessionPayload on success, null on hard failure.
 * Sets expired=true if the token is expired (vs invalid).
 */
export async function verifySession(token: string): Promise<SessionPayload | null> {
  try {
    const decoded = await getAuth(getAdminApp()).verifyIdToken(token);
    return {
      uid:     decoded.uid,
      email:   decoded.email,
      role:    decoded.role as string | undefined,
      expired: false,
    };
  } catch (err: any) {
    if (err?.code === "auth/id-token-expired") {
      return { uid: "", email: undefined, role: undefined, expired: true };
    }
    return null;
  }
}

/**
 * Creates a long-lived Firebase session cookie from a short-lived ID token.
 * This is what should be stored in __session, not the raw ID token.
 */
export async function createFirebaseSessionCookie(idToken: string, expiresInMs: number): Promise<string> {
  return getAuth(getAdminApp()).createSessionCookie(idToken, { expiresIn: expiresInMs });
}

/**
 * Verifies a Firebase session cookie (not an ID token).
 * Use this for middleware verification of the __session cookie.
 */
export async function verifySessionCookie(cookie: string): Promise<SessionPayload | null> {
  try {
    const decoded = await getAuth(getAdminApp()).verifySessionCookie(cookie, true);
    return {
      expired: false,
      uid:     decoded.uid,
      email:   decoded.email,
      role:    decoded.role as string | undefined,
    };
  } catch (err: any) {
    if (err?.code === "auth/session-cookie-expired") {
      return { expired: true };
    }
    return null;
  }
}
