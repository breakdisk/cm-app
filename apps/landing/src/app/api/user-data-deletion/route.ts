import { NextRequest, NextResponse } from "next/server";
import crypto from "crypto";

// Meta Data Deletion Callback endpoint.
// Meta POSTs here when a user removes the app from Facebook Settings.
// Spec: https://developers.facebook.com/docs/development/create-an-app/app-dashboard/data-deletion-callback
//
// Expected POST body: signed_request=<base64url>.<base64url>
// Expected JSON response: { url: string, confirmation_code: string }

const APP_SECRET = process.env.FACEBOOK_APP_SECRET ?? "";
const BASE_URL = process.env.NEXT_PUBLIC_BASE_URL ?? "https://cargomarket.net";

function base64UrlDecode(str: string): string {
  return Buffer.from(str.replace(/-/g, "+").replace(/_/g, "/"), "base64").toString("utf8");
}

function verifySignedRequest(signedRequest: string): { user_id?: string; issued_at?: number } | null {
  const [encodedSig, payload] = signedRequest.split(".");
  if (!encodedSig || !payload) return null;

  if (APP_SECRET) {
    const expectedSig = crypto
      .createHmac("sha256", APP_SECRET)
      .update(payload)
      .digest("base64")
      .replace(/\+/g, "-")
      .replace(/\//g, "_")
      .replace(/=+$/, "");

    if (encodedSig !== expectedSig) return null;
  }

  try {
    return JSON.parse(base64UrlDecode(payload));
  } catch {
    return null;
  }
}

function generateConfirmationCode(userId: string): string {
  const seed = `${userId}-${Date.now()}-${APP_SECRET || "cargomarket"}`;
  return crypto.createHash("sha256").update(seed).digest("hex").slice(0, 16).toUpperCase();
}

export async function POST(req: NextRequest) {
  try {
    const body = await req.text();
    const params = new URLSearchParams(body);
    const signedRequest = params.get("signed_request");

    if (!signedRequest) {
      return NextResponse.json({ error: "Missing signed_request" }, { status: 400 });
    }

    const data = verifySignedRequest(signedRequest);
    if (!data) {
      return NextResponse.json({ error: "Invalid signed_request" }, { status: 400 });
    }

    const userId = data.user_id ?? "unknown";
    const confirmationCode = generateConfirmationCode(userId);

    // In production: enqueue a deletion job to the identity service here.
    // The identity service processes the request asynchronously, scrubbing
    // all records tied to this Meta user_id within 30 days.
    // Example: await enqueueMetaDeletion({ userId, confirmationCode, requestedAt: new Date() });

    const statusUrl = `${BASE_URL}/user-data-deletion?code=${confirmationCode}`;

    return NextResponse.json({
      url: statusUrl,
      confirmation_code: confirmationCode,
    });
  } catch {
    return NextResponse.json({ error: "Internal server error" }, { status: 500 });
  }
}

// Meta also sends a GET to verify the endpoint is reachable.
export async function GET() {
  return NextResponse.json({
    status: "ok",
    endpoint: "CargoMarket Meta Data Deletion Callback",
    instructions_url: `${BASE_URL}/user-data-deletion`,
  });
}
