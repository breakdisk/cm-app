import { NextRequest, NextResponse } from "next/server";
import { verifySession } from "@/lib/firebase/admin";
import { exchangeFirebaseToken, IdentityError } from "@/lib/identity-client";
import { setLosCookies, setSessionCookie } from "@/lib/los-cookies";

/**
 * Server-side sign-in handoff.
 *
 * Flow:
 *   1. Client (login page) finishes Firebase sign-in, gets an `idToken`.
 *   2. Client POSTs {idToken, role, partnerSlug?, partnerSig?} here.
 *   3. We verify the idToken against Firebase Admin.
 *   4. We hit identity `/v1/internal/auth/exchange-firebase` with the verified
 *      user context to mint a LogisticOS JWT pair. Identity handles lazy
 *      draft-tenant provisioning for first-time merchants.
 *   5. We set three cookies on the landing domain: `__session` (Firebase),
 *      `__los_at` (LogisticOS access), `__los_rt` (LogisticOS refresh).
 *   6. We return `{ ok, onboarding_required, tenant_slug }` so the client can
 *      decide whether to redirect to `/setup` or the portal home.
 *
 * The LogisticOS tokens are httpOnly — portals read them via their own
 * `/api/token` route that proxies to the refresh path if expired.
 */

interface Body {
  idToken: string;
  role: string;
  partnerSlug?: string;
  partnerSig?: string;
}

export async function POST(req: NextRequest) {
  const body = (await req.json()) as Body;

  if (!body.idToken || !body.role) {
    return NextResponse.json({ error: "Missing idToken or role" }, { status: 400 });
  }

  const session = await verifySession(body.idToken);
  if (!session) {
    return NextResponse.json({ error: "Invalid token" }, { status: 401 });
  }
  if (session.expired) {
    return NextResponse.json({ error: "Token expired" }, { status: 401 });
  }

  // Enforce role claim matches requested portal (only if claim is already set)
  if (session.role && session.role !== body.role) {
    return NextResponse.json({ error: "Unauthorized role" }, { status: 403 });
  }

  if (!session.email) {
    return NextResponse.json({ error: "Firebase account missing email" }, { status: 400 });
  }

  // Exchange the verified Firebase identity for a LogisticOS JWT pair.
  try {
    const exchange = await exchangeFirebaseToken({
      firebase_uid:   session.uid,
      email:          session.email,
      email_verified: true, // Firebase Admin already verified the idToken
      role:           body.role,
      partner_slug:   body.partnerSlug,
      partner_sig:    body.partnerSig,
    });

    const res = NextResponse.json({
      ok: true,
      onboarding_required: exchange.user.onboarding_required,
      tenant_slug:         exchange.user.tenant_slug,
    });
    setSessionCookie(res, body.idToken);
    setLosCookies(res, exchange);
    return res;
  } catch (err) {
    if (err instanceof IdentityError) {
      console.error("[session] identity exchange failed", err.status, err.code, err.message);
      // Never forward raw internal errors (e.g., missing env vars) to the browser.
      const clientMessage =
        err.status >= 500
          ? "Service temporarily unavailable. Please try again."
          : err.message;
      return NextResponse.json(
        { error: clientMessage, code: err.code },
        { status: err.status === 500 ? 502 : err.status },
      );
    }
    console.error("[session] identity exchange threw", err);
    return NextResponse.json(
      { error: "Service temporarily unavailable. Please try again." },
      { status: 502 },
    );
  }
}
