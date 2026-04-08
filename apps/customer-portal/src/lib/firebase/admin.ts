import { initializeApp, getApps, cert, type App } from "firebase-admin/app";
import { getAuth } from "firebase-admin/auth";

const serviceAccount = process.env.FIREBASE_SERVICE_ACCOUNT_JSON
  ? JSON.parse(Buffer.from(process.env.FIREBASE_SERVICE_ACCOUNT_JSON, "base64").toString("utf8"))
  : null;

function getAdminApp(): App {
  if (getApps().length) return getApps()[0];
  if (!serviceAccount) throw new Error("FIREBASE_SERVICE_ACCOUNT_JSON env var is missing or invalid");
  return initializeApp({ credential: cert(serviceAccount) });
}

export interface SessionPayload {
  uid:   string;
  email: string | undefined;
  role:  string | undefined;
}

export async function verifySessionCookie(cookie: string): Promise<SessionPayload | null> {
  try {
    const decoded = await getAuth(getAdminApp()).verifySessionCookie(cookie, true);
    return { uid: decoded.uid, email: decoded.email, role: decoded.role as string | undefined };
  } catch {
    return null;
  }
}
