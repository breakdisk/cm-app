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
  uid:     string;
  email:   string | undefined;
  role:    string | undefined;
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
      role:    decoded["role"] as string | undefined,
      expired: false,
    };
  } catch (err: any) {
    if (err?.code === "auth/id-token-expired") {
      return { uid: "", email: undefined, role: undefined, expired: true };
    }
    return null;
  }
}
