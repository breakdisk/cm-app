# Single-Domain Multi-Portal Firebase Auth Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Serve all LogisticOS portals from `os.cargomarket.net` with centralized Firebase Auth, role-based routing, and a public tracking page.

**Architecture:** The landing app (`apps/landing`) acts as the single public entry point — it proxies `/merchant/*`, `/admin/*`, `/partner/*`, `/customer/*` to internal portal containers via Next.js rewrites. Firebase Auth issues ID tokens which are stored as httpOnly cookies; middleware on both landing and each portal verifies the token and role on every request.

**Tech Stack:** Next.js 14 App Router, TypeScript, Firebase Auth (JS client SDK v10), firebase-admin (Node.js), httpOnly cookies, dark glassmorphism design system (Tailwind + Framer Motion already in place).

---

## File Map

### Landing (`apps/landing`)

| File | Action | Purpose |
|------|--------|---------|
| `next.config.mjs` | Modify | Add rewrites + firebase-admin serverExternalPackages |
| `src/middleware.ts` | Create | Protect `/merchant`, `/admin`, `/partner`, `/customer` paths |
| `src/lib/firebase/client.ts` | Create | Firebase client SDK init (singleton) |
| `src/lib/firebase/admin.ts` | Create | Firebase Admin SDK init + verifySession() |
| `src/app/api/auth/session/route.ts` | Create | POST: verify ID token → set `__session` cookie |
| `src/app/api/auth/signout/route.ts` | Create | POST: clear `__session` cookie → redirect `/` |
| `src/app/login/page.tsx` | Create | Role selector + Firebase social/magic-link auth UI |
| `src/app/track/page.tsx` | Create | Public AWB search + shipment timeline |
| `src/components/Navbar.tsx` | Modify | "Sign in" → `/login`, add "Track" nav link |
| `.env.local.example` | Create | Document required env vars |

### Each Portal (merchant / admin / partner / customer)

| File | Action | Purpose |
|------|--------|---------|
| `next.config.mjs` | Modify | Add `basePath`, `serverExternalPackages` |
| `src/middleware.ts` | Create | Verify `__session` cookie, redirect to landing `/login?role=x` |
| `src/lib/firebase/admin.ts` | Create | Firebase Admin SDK init + verifySession() |
| `src/app/(auth)/login/` | Delete | Auth is now centralized |
| `src/app/(auth)/register/` | Delete | Registration via Firebase on landing |

---

## Task 1: Firebase project setup + env vars

**Files:**
- Create: `apps/landing/.env.local.example`
- Create: `apps/merchant-portal/.env.local.example`
- Create: `apps/admin-portal/.env.local.example`
- Create: `apps/partner-portal/.env.local.example`
- Create: `apps/customer-portal/.env.local.example`

- [ ] **Step 1: Create a Firebase project**

  1. Go to [https://console.firebase.google.com](https://console.firebase.google.com) → "Add project" → name it `logisticos-prod`
  2. In **Authentication** → **Sign-in method**, enable: Google, Facebook, Email/Password (for magic link)
  3. Enable **Email link (passwordless sign-in)** under Email/Password provider
  4. In **Project settings** → **General** → scroll to "Your apps" → Add a **Web app** → copy the config object
  5. In **Project settings** → **Service accounts** → "Generate new private key" → save as `service-account.json`
  6. Base64-encode it: `base64 -w 0 service-account.json` (Linux/Mac) or `[Convert]::ToBase64String([IO.File]::ReadAllBytes('service-account.json'))` (PowerShell)

- [ ] **Step 2: Create env example files**

  Create `apps/landing/.env.local.example`:
  ```env
  # Firebase Client (public — safe to expose)
  NEXT_PUBLIC_FIREBASE_API_KEY=AIza...
  NEXT_PUBLIC_FIREBASE_AUTH_DOMAIN=logisticos-prod.firebaseapp.com
  NEXT_PUBLIC_FIREBASE_PROJECT_ID=logisticos-prod
  NEXT_PUBLIC_FIREBASE_APP_ID=1:123:web:abc

  # Firebase Admin (server only — never expose)
  FIREBASE_SERVICE_ACCOUNT_JSON=eyJ0eXBlIjoic2VydmljZa...   # base64-encoded JSON

  # Portal internal container URLs (server only)
  MERCHANT_PORTAL_URL=http://logisticos-merchant:3000
  ADMIN_PORTAL_URL=http://logisticos-admin:3001
  PARTNER_PORTAL_URL=http://logisticos-partner:3003
  CUSTOMER_PORTAL_URL=http://logisticos-customer:3002
  ```

  Create `apps/merchant-portal/.env.local.example` (same Firebase vars, no portal URLs):
  ```env
  NEXT_PUBLIC_FIREBASE_API_KEY=AIza...
  NEXT_PUBLIC_FIREBASE_AUTH_DOMAIN=logisticos-prod.firebaseapp.com
  NEXT_PUBLIC_FIREBASE_PROJECT_ID=logisticos-prod
  NEXT_PUBLIC_FIREBASE_APP_ID=1:123:web:abc
  FIREBASE_SERVICE_ACCOUNT_JSON=eyJ0eXBlIjoic2Vydmljza...
  LANDING_URL=https://os.cargomarket.net
  ```

  Copy the same `.env.local.example` to `apps/admin-portal/`, `apps/partner-portal/`, `apps/customer-portal/` — identical content.

- [ ] **Step 3: Copy `.env.local.example` to `.env.local` in each app and fill in real values**

  ```bash
  cp apps/landing/.env.local.example apps/landing/.env.local
  cp apps/merchant-portal/.env.local.example apps/merchant-portal/.env.local
  cp apps/admin-portal/.env.local.example apps/admin-portal/.env.local
  cp apps/partner-portal/.env.local.example apps/partner-portal/.env.local
  cp apps/customer-portal/.env.local.example apps/customer-portal/.env.local
  ```

  Edit each `.env.local` with real Firebase values from the console.

- [ ] **Step 4: Add `.env.local` to `.gitignore` (if not already)**

  ```bash
  grep -q ".env.local" .gitignore || echo ".env.local" >> .gitignore
  ```

- [ ] **Step 5: Commit**

  ```bash
  git add apps/landing/.env.local.example apps/merchant-portal/.env.local.example \
          apps/admin-portal/.env.local.example apps/partner-portal/.env.local.example \
          apps/customer-portal/.env.local.example .gitignore
  git commit -m "chore: add Firebase env var examples for all apps"
  ```

---

## Task 2: Install Firebase dependencies

**Files:**
- Modify: `apps/landing/package.json`
- Modify: `apps/merchant-portal/package.json`
- Modify: `apps/admin-portal/package.json`
- Modify: `apps/partner-portal/package.json`
- Modify: `apps/customer-portal/package.json`

- [ ] **Step 1: Install in landing**

  ```bash
  cd apps/landing
  npm install firebase firebase-admin
  cd ../..
  ```

  Expected: `firebase` (client SDK) and `firebase-admin` added to `dependencies` in `apps/landing/package.json`.

- [ ] **Step 2: Install firebase-admin in each portal**

  ```bash
  cd apps/merchant-portal && npm install firebase-admin && cd ../..
  cd apps/admin-portal    && npm install firebase-admin && cd ../..
  cd apps/partner-portal  && npm install firebase-admin && cd ../..
  cd apps/customer-portal && npm install firebase-admin && cd ../..
  ```

- [ ] **Step 3: Commit**

  ```bash
  git add apps/landing/package.json apps/landing/package-lock.json \
          apps/merchant-portal/package.json apps/merchant-portal/package-lock.json \
          apps/admin-portal/package.json apps/admin-portal/package-lock.json \
          apps/partner-portal/package.json apps/partner-portal/package-lock.json \
          apps/customer-portal/package.json apps/customer-portal/package-lock.json
  git commit -m "chore: install firebase and firebase-admin dependencies"
  ```

---

## Task 3: Firebase client + admin SDK helpers (landing)

**Files:**
- Create: `apps/landing/src/lib/firebase/client.ts`
- Create: `apps/landing/src/lib/firebase/admin.ts`

- [ ] **Step 1: Create Firebase client singleton**

  Create `apps/landing/src/lib/firebase/client.ts`:
  ```typescript
  import { initializeApp, getApps, getApp, type FirebaseApp } from "firebase/app";

  const firebaseConfig = {
    apiKey:    process.env.NEXT_PUBLIC_FIREBASE_API_KEY!,
    authDomain: process.env.NEXT_PUBLIC_FIREBASE_AUTH_DOMAIN!,
    projectId: process.env.NEXT_PUBLIC_FIREBASE_PROJECT_ID!,
    appId:     process.env.NEXT_PUBLIC_FIREBASE_APP_ID!,
  };

  export function getFirebaseApp(): FirebaseApp {
    return getApps().length ? getApp() : initializeApp(firebaseConfig);
  }
  ```

- [ ] **Step 2: Create Firebase Admin singleton + verifySession**

  Create `apps/landing/src/lib/firebase/admin.ts`:
  ```typescript
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
  ```

- [ ] **Step 3: Commit**

  ```bash
  git add apps/landing/src/lib/firebase/
  git commit -m "feat(landing): add Firebase client and admin SDK helpers"
  ```

---

## Task 4: Session API routes (landing)

**Files:**
- Create: `apps/landing/src/app/api/auth/session/route.ts`
- Create: `apps/landing/src/app/api/auth/signout/route.ts`

- [ ] **Step 1: Create session route (sets httpOnly cookie)**

  Create `apps/landing/src/app/api/auth/session/route.ts`:
  ```typescript
  import { NextRequest, NextResponse } from "next/server";
  import { verifySession } from "@/lib/firebase/admin";

  const SEVEN_DAYS = 60 * 60 * 24 * 7;

  export async function POST(req: NextRequest) {
    const { idToken, role } = await req.json() as { idToken: string; role: string };

    if (!idToken || !role) {
      return NextResponse.json({ error: "Missing idToken or role" }, { status: 400 });
    }

    const session = await verifySession(idToken);
    if (!session) {
      return NextResponse.json({ error: "Invalid token" }, { status: 401 });
    }

    // Enforce role claim matches the requested portal
    if (session.role && session.role !== role) {
      return NextResponse.json({ error: "Unauthorized role" }, { status: 403 });
    }

    const res = NextResponse.json({ ok: true });
    res.cookies.set("__session", idToken, {
      httpOnly: true,
      secure:   process.env.NODE_ENV === "production",
      sameSite: "lax",
      maxAge:   SEVEN_DAYS,
      path:     "/",
    });
    return res;
  }
  ```

- [ ] **Step 2: Create signout route**

  Create `apps/landing/src/app/api/auth/signout/route.ts`:
  ```typescript
  import { NextResponse } from "next/server";

  export async function POST() {
    const res = NextResponse.redirect(
      new URL("/", process.env.NEXT_PUBLIC_FIREBASE_AUTH_DOMAIN
        ? `https://${process.env.NEXT_PUBLIC_FIREBASE_AUTH_DOMAIN.replace(".firebaseapp.com", "")}.cargomarket.net`
        : "http://localhost:3004"
      )
    );
    res.cookies.set("__session", "", { maxAge: 0, path: "/" });
    return res;
  }
  ```

- [ ] **Step 3: Commit**

  ```bash
  git add apps/landing/src/app/api/auth/
  git commit -m "feat(landing): add session and signout API routes"
  ```

---

## Task 5: Landing middleware (protect portal paths)

**Files:**
- Create: `apps/landing/src/middleware.ts`

- [ ] **Step 1: Create middleware**

  Create `apps/landing/src/middleware.ts`:
  ```typescript
  import { NextRequest, NextResponse } from "next/server";
  import { verifySession } from "@/lib/firebase/admin";

  const PORTAL_ROLES: Record<string, string> = {
    merchant: "merchant",
    admin:    "admin",
    partner:  "partner",
    customer: "customer",
  };

  export async function middleware(req: NextRequest) {
    const { pathname } = req.nextUrl;

    // Extract portal prefix (e.g. "merchant" from "/merchant/dashboard")
    const prefix = pathname.split("/")[1];
    const requiredRole = PORTAL_ROLES[prefix];

    // Not a protected path — allow through
    if (!requiredRole) return NextResponse.next();

    const token = req.cookies.get("__session")?.value;
    if (!token) return redirectToLogin(req, prefix);

    const session = await verifySession(token);
    if (!session) return redirectToLogin(req, prefix);

    // Expired token — redirect with expired flag so login page shows message
    if (session.expired) return redirectToLogin(req, prefix, "expired");

    // If user has no role claim yet, allow access (claim set after first login)
    if (session.role && session.role !== requiredRole) {
      return redirectToLogin(req, prefix, "unauthorized");
    }

    return NextResponse.next();
  }

  function redirectToLogin(req: NextRequest, role: string, error?: string) {
    const url = req.nextUrl.clone();
    url.pathname = "/login";
    url.searchParams.set("role", role);
    if (error) url.searchParams.set("error", error);
    return NextResponse.redirect(url);
  }

  export const config = {
    matcher: ["/merchant/:path*", "/admin/:path*", "/partner/:path*", "/customer/:path*"],
  };
  ```

- [ ] **Step 2: Commit**

  ```bash
  git add apps/landing/src/middleware.ts
  git commit -m "feat(landing): add middleware to protect portal paths"
  ```

---

## Task 6: Landing next.config — rewrites + firebase-admin

**Files:**
- Modify: `apps/landing/next.config.mjs`

- [ ] **Step 1: Update next.config.mjs**

  Replace the full contents of `apps/landing/next.config.mjs`:
  ```javascript
  /** @type {import('next').NextConfig} */
  const nextConfig = {
    output: "standalone",
    serverExternalPackages: ["firebase-admin"],

    async rewrites() {
      const merchantUrl = process.env.MERCHANT_PORTAL_URL ?? "http://localhost:3000";
      const adminUrl    = process.env.ADMIN_PORTAL_URL    ?? "http://localhost:3001";
      const partnerUrl  = process.env.PARTNER_PORTAL_URL  ?? "http://localhost:3003";
      const customerUrl = process.env.CUSTOMER_PORTAL_URL ?? "http://localhost:3002";

      return [
        {
          source:      "/merchant/:path*",
          destination: `${merchantUrl}/merchant/:path*`,
        },
        {
          source:      "/admin/:path*",
          destination: `${adminUrl}/admin/:path*`,
        },
        {
          source:      "/partner/:path*",
          destination: `${partnerUrl}/partner/:path*`,
        },
        {
          source:      "/customer/:path*",
          destination: `${customerUrl}/customer/:path*`,
        },
      ];
    },
  };

  export default nextConfig;
  ```

- [ ] **Step 2: Verify rewrites don't conflict with `/login` and `/track` routes**

  The rewrites only match `/merchant/*`, `/admin/*`, `/partner/*`, `/customer/*`. Routes `/login` and `/track` are handled by the landing app itself. No conflict.

- [ ] **Step 3: Commit**

  ```bash
  git add apps/landing/next.config.mjs
  git commit -m "feat(landing): add Next.js rewrites for portal proxying"
  ```

---

## Task 7: Login page (landing)

**Files:**
- Create: `apps/landing/src/app/login/page.tsx`

- [ ] **Step 1: Create the login page**

  Create `apps/landing/src/app/login/page.tsx`:
  ```typescript
  "use client";

  import { useEffect, useState } from "react";
  import { useRouter, useSearchParams } from "next/navigation";
  import { motion, AnimatePresence } from "framer-motion";
  import {
    getFirebaseApp,
  } from "@/lib/firebase/client";
  import {
    getAuth,
    GoogleAuthProvider,
    FacebookAuthProvider,
    signInWithPopup,
    sendSignInLinkToEmail,
    isSignInWithEmailLink,
    signInWithEmailLink,
  } from "firebase/auth";
  import { Building2, ShieldCheck, Handshake, User, Loader2, Mail, Zap } from "lucide-react";

  const ROLES = [
    {
      id:          "merchant",
      label:       "Merchant",
      description: "Ship & manage orders",
      icon:        Building2,
      accent:      "#00E5FF",
      glow:        "shadow-glow-cyan",
      border:      "border-cyan-neon/30",
      bg:          "bg-cyan-neon/5",
    },
    {
      id:          "admin",
      label:       "Admin",
      description: "Operations & dispatch",
      icon:        ShieldCheck,
      accent:      "#A855F7",
      glow:        "shadow-glow-purple",
      border:      "border-purple-plasma/30",
      bg:          "bg-purple-plasma/5",
    },
    {
      id:          "partner",
      label:       "Partner",
      description: "Carrier & SLA dashboard",
      icon:        Handshake,
      accent:      "#FFAB00",
      glow:        "shadow-glow-amber",
      border:      "border-amber-signal/30",
      bg:          "bg-amber-signal/5",
    },
    {
      id:          "customer",
      label:       "Customer",
      description: "Track your shipments",
      icon:        User,
      accent:      "#00FF88",
      glow:        "shadow-glow-green",
      border:      "border-green-signal/30",
      bg:          "bg-green-signal/5",
    },
  ] as const;

  type RoleId = (typeof ROLES)[number]["id"];

  function GoogleIcon() {
    return (
      <svg viewBox="0 0 24 24" className="h-4 w-4" aria-hidden="true">
        <path d="M22.56 12.25c0-.78-.07-1.53-.2-2.25H12v4.26h5.92c-.26 1.37-1.04 2.53-2.21 3.31v2.77h3.57c2.08-1.92 3.28-4.74 3.28-8.09z" fill="#4285F4"/>
        <path d="M12 23c2.97 0 5.46-.98 7.28-2.66l-3.57-2.77c-.98.66-2.23 1.06-3.71 1.06-2.86 0-5.29-1.93-6.16-4.53H2.18v2.84C3.99 20.53 7.7 23 12 23z" fill="#34A853"/>
        <path d="M5.84 14.09c-.22-.66-.35-1.36-.35-2.09s.13-1.43.35-2.09V7.07H2.18C1.43 8.55 1 10.22 1 12s.43 3.45 1.18 4.93l2.85-2.22.81-.62z" fill="#FBBC05"/>
        <path d="M12 5.38c1.62 0 3.06.56 4.21 1.64l3.15-3.15C17.45 2.09 14.97 1 12 1 7.7 1 3.99 3.47 2.18 7.07l3.66 2.84c.87-2.6 3.3-4.53 6.16-4.53z" fill="#EA4335"/>
      </svg>
    );
  }

  export default function LoginPage() {
    const router       = useRouter();
    const searchParams = useSearchParams();
    const [selectedRole, setSelectedRole] = useState<RoleId | null>(
      (searchParams.get("role") as RoleId) ?? null
    );
    const [loading, setLoading]   = useState(false);
    const [email, setEmail]       = useState("");
    const [emailSent, setEmailSent] = useState(false);
    const errorParam = searchParams.get("error");
    const [error, setError]       = useState<string | null>(
      errorParam === "unauthorized" ? "You don't have access to that portal." :
      errorParam === "expired"      ? "Your session expired. Please sign in again." :
      null
    );
    const [showMagic, setShowMagic] = useState(false);

    const auth = getAuth(getFirebaseApp());

    // Handle magic link return
    useEffect(() => {
      if (isSignInWithEmailLink(auth, window.location.href)) {
        const savedEmail = window.localStorage.getItem("emailForSignIn");
        if (!savedEmail) return;
        setLoading(true);
        signInWithEmailLink(auth, savedEmail, window.location.href)
          .then((result) => result.user.getIdToken())
          .then((idToken) => completeSignIn(idToken))
          .catch(() => setError("Magic link sign-in failed. Please try again."))
          .finally(() => setLoading(false));
      }
    }, []);

    async function completeSignIn(idToken: string) {
      const role = selectedRole ?? (searchParams.get("role") as RoleId) ?? "customer";
      const res  = await fetch("/api/auth/session", {
        method:  "POST",
        headers: { "Content-Type": "application/json" },
        body:    JSON.stringify({ idToken, role }),
      });
      if (!res.ok) {
        const { error } = await res.json();
        setError(error ?? "Sign-in failed.");
        return;
      }
      router.push(`/${role}`);
    }

    async function signInWithGoogle() {
      if (!selectedRole) return;
      setLoading(true);
      setError(null);
      try {
        const result  = await signInWithPopup(auth, new GoogleAuthProvider());
        const idToken = await result.user.getIdToken();
        await completeSignIn(idToken);
      } catch {
        setError("Google sign-in failed. Please try again.");
      } finally {
        setLoading(false);
      }
    }

    async function signInWithFacebook() {
      if (!selectedRole) return;
      setLoading(true);
      setError(null);
      try {
        const result  = await signInWithPopup(auth, new FacebookAuthProvider());
        const idToken = await result.user.getIdToken();
        await completeSignIn(idToken);
      } catch {
        setError("Facebook sign-in failed. Please try again.");
      } finally {
        setLoading(false);
      }
    }

    async function sendMagicLink() {
      if (!email || !selectedRole) return;
      setLoading(true);
      setError(null);
      try {
        window.localStorage.setItem("emailForSignIn", email);
        await sendSignInLinkToEmail(auth, email, {
          url:             `${window.location.origin}/login?role=${selectedRole}`,
          handleCodeInApp: true,
        });
        setEmailSent(true);
      } catch {
        setError("Failed to send magic link. Please try again.");
      } finally {
        setLoading(false);
      }
    }

    return (
      <div className="min-h-screen bg-[#050810] flex items-center justify-center px-4">
        {/* Background grid */}
        <div
          className="pointer-events-none fixed inset-0 opacity-30"
          style={{
            backgroundImage: "linear-gradient(rgba(0,229,255,0.04) 1px, transparent 1px), linear-gradient(90deg, rgba(0,229,255,0.04) 1px, transparent 1px)",
            backgroundSize:  "48px 48px",
          }}
        />

        <motion.div
          initial={{ opacity: 0, y: 24 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.5, ease: [0.16, 1, 0.3, 1] }}
          className="w-full max-w-lg"
        >
          {/* Logo */}
          <a href="/" className="flex items-center gap-2.5 justify-center mb-8">
            <div className="relative w-8 h-8 flex items-center justify-center">
              <div className="absolute inset-0 rounded-lg bg-gradient-to-br from-cyan-neon/30 to-purple-plasma/30" />
              <Zap className="w-4 h-4 text-cyan-neon relative z-10" strokeWidth={2.5} />
            </div>
            <span className="text-lg font-bold tracking-tight" style={{ fontFamily: "'Space Grotesk', sans-serif" }}>
              <span className="bg-gradient-to-r from-cyan-neon via-purple-plasma to-green-signal bg-clip-text text-transparent">Cargo</span>
              <span className="text-white">Market</span>
            </span>
          </a>

          <div className="rounded-2xl border border-white/[0.08] bg-white/[0.03] backdrop-blur-xl p-8 shadow-glass-lg">
            <h1 className="text-2xl font-bold text-white mb-1" style={{ fontFamily: "'Space Grotesk', sans-serif" }}>
              Sign in
            </h1>
            <p className="text-sm text-white/40 mb-6">Select your portal to continue</p>

            {error && (
              <div className="mb-4 rounded-xl border border-red-500/20 bg-red-500/10 px-4 py-3 text-sm text-red-400">
                {error}
              </div>
            )}

            {/* Role cards */}
            <div className="grid grid-cols-2 gap-3 mb-6">
              {ROLES.map((role) => {
                const Icon     = role.icon;
                const selected = selectedRole === role.id;
                return (
                  <button
                    key={role.id}
                    onClick={() => { setSelectedRole(role.id); setShowMagic(false); setEmailSent(false); }}
                    className={`
                      relative rounded-xl border p-4 text-left transition-all duration-200
                      ${selected ? `${role.border} ${role.bg} ${role.glow}` : "border-white/[0.06] bg-white/[0.02] hover:border-white/[0.12] hover:bg-white/[0.04]"}
                    `}
                  >
                    <Icon className="w-5 h-5 mb-2" style={{ color: role.accent }} />
                    <div className="text-sm font-semibold text-white">{role.label}</div>
                    <div className="text-xs text-white/40 mt-0.5">{role.description}</div>
                    {selected && (
                      <div className="absolute top-2 right-2 w-2 h-2 rounded-full" style={{ backgroundColor: role.accent }} />
                    )}
                  </button>
                );
              })}
            </div>

            {/* Auth buttons — shown when role selected */}
            <AnimatePresence>
              {selectedRole && (
                <motion.div
                  initial={{ opacity: 0, height: 0 }}
                  animate={{ opacity: 1, height: "auto" }}
                  exit={{ opacity: 0, height: 0 }}
                  transition={{ duration: 0.25 }}
                  className="overflow-hidden"
                >
                  <div className="flex flex-col gap-3">
                    {/* Google */}
                    <button
                      onClick={signInWithGoogle}
                      disabled={loading}
                      className="flex items-center justify-center gap-3 rounded-xl border border-white/[0.08] bg-white/[0.04] px-4 py-3 text-sm font-medium text-white/70 hover:bg-white/[0.08] hover:text-white transition-all duration-200 disabled:opacity-50"
                    >
                      {loading ? <Loader2 className="h-4 w-4 animate-spin" /> : <GoogleIcon />}
                      Continue with Google
                    </button>

                    {/* Magic link */}
                    {!showMagic ? (
                      <button
                        onClick={() => setShowMagic(true)}
                        className="flex items-center justify-center gap-3 rounded-xl border border-white/[0.08] bg-white/[0.04] px-4 py-3 text-sm font-medium text-white/70 hover:bg-white/[0.08] hover:text-white transition-all duration-200"
                      >
                        <Mail className="h-4 w-4" />
                        Continue with Email Link
                      </button>
                    ) : emailSent ? (
                      <div className="rounded-xl border border-green-signal/20 bg-green-signal/5 px-4 py-3 text-sm text-green-signal text-center">
                        Check your email for the sign-in link.
                      </div>
                    ) : (
                      <div className="flex gap-2">
                        <input
                          type="email"
                          value={email}
                          onChange={(e) => setEmail(e.target.value)}
                          placeholder="you@example.com"
                          className="flex-1 rounded-xl border border-white/[0.08] bg-white/[0.04] px-4 py-3 text-sm text-white placeholder:text-white/20 outline-none focus:border-cyan-neon/40"
                        />
                        <button
                          onClick={sendMagicLink}
                          disabled={loading || !email}
                          className="rounded-xl bg-cyan-neon px-4 py-3 text-sm font-semibold text-[#050810] hover:shadow-glow-cyan transition-all duration-200 disabled:opacity-50"
                        >
                          {loading ? <Loader2 className="h-4 w-4 animate-spin" /> : "Send"}
                        </button>
                      </div>
                    )}
                  </div>
                </motion.div>
              )}
            </AnimatePresence>

            <div className="mt-6 text-center">
              <a href="/track" className="text-xs text-white/30 hover:text-white/60 transition-colors">
                Track a shipment without signing in →
              </a>
            </div>
          </div>
        </motion.div>
      </div>
    );
  }
  ```

- [ ] **Step 2: Verify the page builds**

  ```bash
  cd apps/landing && npm run build 2>&1 | tail -20
  ```

  Expected: build completes (may have warnings, no errors on `/login`).

- [ ] **Step 3: Commit**

  ```bash
  git add apps/landing/src/app/login/
  git commit -m "feat(landing): add centralized login page with role selector and Firebase auth"
  ```

---

## Task 8: Public tracking page (landing)

**Files:**
- Create: `apps/landing/src/app/track/page.tsx`

- [ ] **Step 1: Create the tracking page**

  Create `apps/landing/src/app/track/page.tsx`:
  ```typescript
  "use client";

  import { useState } from "react";
  import { motion, AnimatePresence } from "framer-motion";
  import { Search, Package, Loader2, MapPin, Clock, CheckCircle2, XCircle, Truck, Zap } from "lucide-react";

  interface TrackingEvent {
    status:    string;
    location:  string;
    timestamp: string;
    note?:     string;
  }

  interface TrackingResult {
    awb:           string;
    status:        string;
    origin:        string;
    destination:   string;
    estimated_delivery?: string;
    events:        TrackingEvent[];
  }

  const STATUS_ICONS: Record<string, React.ElementType> = {
    delivered:          CheckCircle2,
    failed:             XCircle,
    in_transit:         Truck,
    out_for_delivery:   Truck,
    default:            Package,
  };

  export default function TrackPage() {
    const [awb, setAwb]           = useState("");
    const [loading, setLoading]   = useState(false);
    const [result, setResult]     = useState<TrackingResult | null>(null);
    const [error, setError]       = useState<string | null>(null);

    async function handleSearch(e: React.FormEvent) {
      e.preventDefault();
      if (!awb.trim()) return;
      setLoading(true);
      setError(null);
      setResult(null);
      try {
        const res = await fetch(
          `${process.env.NEXT_PUBLIC_API_URL ?? ""}/api/v1/tracking/${awb.trim().toUpperCase()}`
        );
        if (res.status === 404) {
          setError("No shipment found for this tracking number.");
          return;
        }
        if (!res.ok) throw new Error("Server error");
        const data = await res.json();
        setResult(data);
      } catch {
        setError("Unable to fetch tracking info. Please try again.");
      } finally {
        setLoading(false);
      }
    }

    return (
      <div className="min-h-screen bg-[#050810] px-4 py-16">
        <div
          className="pointer-events-none fixed inset-0 opacity-20"
          style={{
            backgroundImage: "linear-gradient(rgba(0,229,255,0.04) 1px, transparent 1px), linear-gradient(90deg, rgba(0,229,255,0.04) 1px, transparent 1px)",
            backgroundSize: "48px 48px",
          }}
        />

        <div className="max-w-2xl mx-auto relative">
          {/* Header */}
          <a href="/" className="flex items-center gap-2.5 mb-12">
            <div className="relative w-7 h-7 flex items-center justify-center">
              <div className="absolute inset-0 rounded-lg bg-gradient-to-br from-cyan-neon/30 to-purple-plasma/30" />
              <Zap className="w-3.5 h-3.5 text-cyan-neon relative z-10" strokeWidth={2.5} />
            </div>
            <span className="text-base font-bold tracking-tight" style={{ fontFamily: "'Space Grotesk', sans-serif" }}>
              <span className="bg-gradient-to-r from-cyan-neon to-purple-plasma bg-clip-text text-transparent">Cargo</span>
              <span className="text-white">Market</span>
            </span>
          </a>

          <h1 className="text-3xl font-bold text-white mb-2" style={{ fontFamily: "'Space Grotesk', sans-serif" }}>
            Track Shipment
          </h1>
          <p className="text-white/40 mb-8">Enter your AWB number to see real-time status</p>

          {/* Search form */}
          <form onSubmit={handleSearch} className="flex gap-3 mb-10">
            <input
              type="text"
              value={awb}
              onChange={(e) => setAwb(e.target.value)}
              placeholder="e.g. LS-A1B2C3D4"
              className="flex-1 rounded-xl border border-white/[0.08] bg-white/[0.04] px-5 py-4 text-base text-white placeholder:text-white/20 outline-none focus:border-cyan-neon/40 focus:bg-white/[0.06] transition-all font-mono"
            />
            <button
              type="submit"
              disabled={loading || !awb.trim()}
              className="rounded-xl bg-gradient-to-r from-cyan-neon to-purple-plasma px-6 py-4 text-[#050810] font-semibold hover:shadow-glow-cyan transition-all duration-300 disabled:opacity-50 flex items-center gap-2"
            >
              {loading ? <Loader2 className="h-5 w-5 animate-spin" /> : <Search className="h-5 w-5" />}
            </button>
          </form>

          {/* Error */}
          {error && (
            <div className="rounded-xl border border-red-500/20 bg-red-500/10 px-5 py-4 text-sm text-red-400 mb-6">
              {error}
            </div>
          )}

          {/* Result */}
          <AnimatePresence>
            {result && (
              <motion.div
                initial={{ opacity: 0, y: 16 }}
                animate={{ opacity: 1, y: 0 }}
                transition={{ duration: 0.4, ease: [0.16, 1, 0.3, 1] }}
              >
                {/* Summary card */}
                <div className="rounded-2xl border border-white/[0.08] bg-white/[0.03] p-6 mb-4">
                  <div className="flex items-start justify-between mb-4">
                    <div>
                      <div className="text-xs text-white/30 mb-1 font-mono">AWB</div>
                      <div className="text-xl font-bold text-white font-mono">{result.awb}</div>
                    </div>
                    <div className="rounded-full border border-cyan-neon/30 bg-cyan-neon/10 px-4 py-1.5 text-sm text-cyan-neon font-semibold capitalize">
                      {result.status.replace(/_/g, " ")}
                    </div>
                  </div>
                  <div className="flex gap-6 text-sm text-white/50">
                    <div className="flex items-center gap-1.5">
                      <MapPin className="h-3.5 w-3.5" />
                      {result.origin} → {result.destination}
                    </div>
                    {result.estimated_delivery && (
                      <div className="flex items-center gap-1.5">
                        <Clock className="h-3.5 w-3.5" />
                        ETA {result.estimated_delivery}
                      </div>
                    )}
                  </div>
                </div>

                {/* Timeline */}
                <div className="rounded-2xl border border-white/[0.08] bg-white/[0.03] p-6">
                  <h2 className="text-sm font-semibold text-white/60 mb-5 uppercase tracking-wider">Tracking History</h2>
                  <div className="relative">
                    <div className="absolute left-[9px] top-2 bottom-2 w-px bg-white/[0.06]" />
                    <div className="flex flex-col gap-5">
                      {result.events.map((event, i) => {
                        const Icon = STATUS_ICONS[event.status] ?? STATUS_ICONS.default;
                        return (
                          <div key={i} className="flex gap-4 relative">
                            <div className="flex-shrink-0 w-[18px] h-[18px] rounded-full border border-cyan-neon/40 bg-cyan-neon/10 flex items-center justify-center mt-0.5 z-10">
                              <Icon className="h-2.5 w-2.5 text-cyan-neon" />
                            </div>
                            <div>
                              <div className="text-sm font-medium text-white capitalize">{event.status.replace(/_/g, " ")}</div>
                              <div className="text-xs text-white/40 mt-0.5">{event.location} · {event.timestamp}</div>
                              {event.note && <div className="text-xs text-white/30 mt-0.5">{event.note}</div>}
                            </div>
                          </div>
                        );
                      })}
                    </div>
                  </div>
                </div>
              </motion.div>
            )}
          </AnimatePresence>

          {/* CTA */}
          <div className="mt-12 text-center">
            <p className="text-sm text-white/30 mb-3">Want to ship with CargoMarket?</p>
            <a
              href="/login?role=merchant"
              className="inline-flex items-center gap-2 rounded-xl bg-gradient-to-r from-cyan-neon to-purple-plasma px-6 py-3 text-sm font-semibold text-[#050810] hover:shadow-glow-cyan transition-all duration-300"
            >
              Get Started Free
            </a>
          </div>
        </div>
      </div>
    );
  }
  ```

- [ ] **Step 2: Commit**

  ```bash
  git add apps/landing/src/app/track/
  git commit -m "feat(landing): add public shipment tracking page"
  ```

---

## Task 9: Update Navbar (landing)

**Files:**
- Modify: `apps/landing/src/components/Navbar.tsx`

- [ ] **Step 1: Update navLinks and CTA buttons**

  In `apps/landing/src/components/Navbar.tsx`, change the `navLinks` array and CTA section:

  Change:
  ```typescript
  const navLinks = [
    { label: "Platform", href: "#platform" },
    { label: "Features", href: "#features" },
    { label: "How It Works", href: "#how-it-works" },
    { label: "AI Engine", href: "#ai" },
    { label: "Pricing", href: "#pricing" },
  ];
  ```

  To:
  ```typescript
  const navLinks = [
    { label: "Platform",    href: "#platform" },
    { label: "Features",    href: "#features" },
    { label: "How It Works", href: "#how-it-works" },
    { label: "AI Engine",   href: "#ai" },
    { label: "Pricing",     href: "#pricing" },
    { label: "Track",       href: "/track" },
  ];
  ```

- [ ] **Step 2: Update the desktop Sign in link**

  Change:
  ```tsx
  <a
    href="#"
    className="px-4 py-2 text-sm text-slate-300 hover:text-white transition-colors duration-200"
  >
    Sign in
  </a>
  ```

  To:
  ```tsx
  <a
    href="/login"
    className="px-4 py-2 text-sm text-slate-300 hover:text-white transition-colors duration-200"
  >
    Sign in
  </a>
  ```

- [ ] **Step 3: Update mobile menu Sign in link**

  In the mobile menu section, add a Sign in link after the nav links:
  ```tsx
  <a
    href="/login"
    onClick={() => setOpen(false)}
    className="py-3 text-sm text-slate-300 hover:text-cyan-neon border-b border-white/[0.04] transition-colors"
  >
    Sign in
  </a>
  ```

- [ ] **Step 4: Commit**

  ```bash
  git add apps/landing/src/components/Navbar.tsx
  git commit -m "feat(landing): update Navbar with Sign in and Track links"
  ```

---

## Task 10: Portal basePath + firebase-admin (merchant-portal)

**Files:**
- Modify: `apps/merchant-portal/next.config.mjs`
- Create: `apps/merchant-portal/src/lib/firebase/admin.ts`
- Create: `apps/merchant-portal/src/middleware.ts`
- Delete: `apps/merchant-portal/src/app/(auth)/login/`
- Delete: `apps/merchant-portal/src/app/(auth)/register/`

- [ ] **Step 1: Update next.config.mjs**

  Replace contents of `apps/merchant-portal/next.config.mjs`:
  ```javascript
  /** @type {import('next').NextConfig} */
  const nextConfig = {
    output: "standalone",
    basePath: "/merchant",
    serverExternalPackages: ["firebase-admin"],
    transpilePackages: ["three"],
    images: {
      remotePatterns: [
        { protocol: "https", hostname: "**.amazonaws.com" },
      ],
    },
    typescript: { ignoreBuildErrors: true },
    eslint:     { ignoreDuringBuilds: true },
  };

  export default nextConfig;
  ```

- [ ] **Step 2: Create Firebase Admin helper**

  Create `apps/merchant-portal/src/lib/firebase/admin.ts`:
  ```typescript
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
    uid:   string;
    email: string | undefined;
    role:  string | undefined;
  }

  export async function verifySession(token: string): Promise<SessionPayload | null> {
    try {
      const decoded = await getAuth(getAdminApp()).verifyIdToken(token);
      return { uid: decoded.uid, email: decoded.email, role: decoded.role as string | undefined };
    } catch {
      return null;
    }
  }
  ```

- [ ] **Step 3: Create portal middleware**

  Create `apps/merchant-portal/src/middleware.ts`:
  ```typescript
  import { NextRequest, NextResponse } from "next/server";
  import { verifySession } from "@/lib/firebase/admin";

  const LANDING_URL = process.env.LANDING_URL ?? "http://localhost:3004";

  export async function middleware(req: NextRequest) {
    const token = req.cookies.get("__session")?.value;
    if (!token) return redirectToLogin(req);

    const session = await verifySession(token);
    if (!session) return redirectToLogin(req);

    if (session.role && session.role !== "merchant") {
      return redirectToLogin(req, "unauthorized");
    }

    return NextResponse.next();
  }

  function redirectToLogin(req: NextRequest, error?: string) {
    const url = new URL(`${LANDING_URL}/login`);
    url.searchParams.set("role", "merchant");
    if (error) url.searchParams.set("error", error);
    return NextResponse.redirect(url);
  }

  export const config = {
    matcher: ["/((?!_next/static|_next/image|favicon.ico).*)"],
  };
  ```

- [ ] **Step 4: Remove centralized auth routes**

  ```bash
  rm -rf apps/merchant-portal/src/app/\(auth\)/login
  rm -rf apps/merchant-portal/src/app/\(auth\)/register
  ```

  If `(auth)` folder is now empty:
  ```bash
  rmdir apps/merchant-portal/src/app/\(auth\) 2>/dev/null || true
  ```

- [ ] **Step 5: Commit**

  ```bash
  git add apps/merchant-portal/next.config.mjs \
          apps/merchant-portal/src/lib/firebase/ \
          apps/merchant-portal/src/middleware.ts
  git rm -r apps/merchant-portal/src/app/\(auth\)/ 2>/dev/null || true
  git commit -m "feat(merchant-portal): add basePath, Firebase middleware, remove local auth"
  ```

---

## Task 11: Portal basePath + firebase-admin (admin-portal)

**Files:**
- Modify: `apps/admin-portal/next.config.mjs`
- Create: `apps/admin-portal/src/lib/firebase/admin.ts`
- Create: `apps/admin-portal/src/middleware.ts`
- Delete: `apps/admin-portal/src/app/(auth)/login/`
- Delete: `apps/admin-portal/src/app/(auth)/register/`

- [ ] **Step 1: Update next.config.mjs**

  Replace contents of `apps/admin-portal/next.config.mjs`:
  ```javascript
  /** @type {import('next').NextConfig} */
  const nextConfig = {
    output: "standalone",
    basePath: "/admin",
    serverExternalPackages: ["firebase-admin"],
    transpilePackages: ["three"],
    images: {
      remotePatterns: [
        { protocol: "https", hostname: "**.amazonaws.com" },
      ],
    },
    typescript: { ignoreBuildErrors: true },
    eslint:     { ignoreDuringBuilds: true },
  };

  export default nextConfig;
  ```

- [ ] **Step 2: Create Firebase Admin helper**

  Create `apps/admin-portal/src/lib/firebase/admin.ts`:
  ```typescript
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
    uid:   string;
    email: string | undefined;
    role:  string | undefined;
  }

  export async function verifySession(token: string): Promise<SessionPayload | null> {
    try {
      const decoded = await getAuth(getAdminApp()).verifyIdToken(token);
      return { uid: decoded.uid, email: decoded.email, role: decoded.role as string | undefined };
    } catch {
      return null;
    }
  }
  ```

- [ ] **Step 3: Create portal middleware**

  Create `apps/admin-portal/src/middleware.ts`:
  ```typescript
  import { NextRequest, NextResponse } from "next/server";
  import { verifySession } from "@/lib/firebase/admin";

  const LANDING_URL = process.env.LANDING_URL ?? "http://localhost:3004";

  export async function middleware(req: NextRequest) {
    const token = req.cookies.get("__session")?.value;
    if (!token) return redirectToLogin(req);

    const session = await verifySession(token);
    if (!session) return redirectToLogin(req);

    if (session.role && session.role !== "admin") {
      return redirectToLogin(req, "unauthorized");
    }

    return NextResponse.next();
  }

  function redirectToLogin(req: NextRequest, error?: string) {
    const url = new URL(`${LANDING_URL}/login`);
    url.searchParams.set("role", "admin");
    if (error) url.searchParams.set("error", error);
    return NextResponse.redirect(url);
  }

  export const config = {
    matcher: ["/((?!_next/static|_next/image|favicon.ico).*)"],
  };
  ```

- [ ] **Step 4: Remove auth routes**

  ```bash
  rm -rf apps/admin-portal/src/app/\(auth\)/login
  rm -rf apps/admin-portal/src/app/\(auth\)/register
  rmdir apps/admin-portal/src/app/\(auth\) 2>/dev/null || true
  ```

- [ ] **Step 5: Commit**

  ```bash
  git add apps/admin-portal/next.config.mjs \
          apps/admin-portal/src/lib/firebase/ \
          apps/admin-portal/src/middleware.ts
  git rm -r apps/admin-portal/src/app/\(auth\)/ 2>/dev/null || true
  git commit -m "feat(admin-portal): add basePath, Firebase middleware, remove local auth"
  ```

---

## Task 12: Portal basePath + firebase-admin (partner-portal)

**Files:**
- Modify: `apps/partner-portal/next.config.mjs`
- Create: `apps/partner-portal/src/lib/firebase/admin.ts`
- Create: `apps/partner-portal/src/middleware.ts`
- Delete: `apps/partner-portal/src/app/(auth)/login/`
- Delete: `apps/partner-portal/src/app/(auth)/register/`

- [ ] **Step 1: Update next.config.mjs**

  Replace contents of `apps/partner-portal/next.config.mjs`:
  ```javascript
  /** @type {import('next').NextConfig} */
  const nextConfig = {
    output: "standalone",
    basePath: "/partner",
    serverExternalPackages: ["firebase-admin"],
    images: {
      remotePatterns: [
        { protocol: "https", hostname: "**.amazonaws.com" },
      ],
    },
    typescript: { ignoreBuildErrors: true },
    eslint:     { ignoreDuringBuilds: true },
  };

  export default nextConfig;
  ```

- [ ] **Step 2: Create Firebase Admin helper**

  Create `apps/partner-portal/src/lib/firebase/admin.ts` — identical to admin-portal version with `role !== "partner"`:
  ```typescript
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
    uid:   string;
    email: string | undefined;
    role:  string | undefined;
  }

  export async function verifySession(token: string): Promise<SessionPayload | null> {
    try {
      const decoded = await getAuth(getAdminApp()).verifyIdToken(token);
      return { uid: decoded.uid, email: decoded.email, role: decoded.role as string | undefined };
    } catch {
      return null;
    }
  }
  ```

- [ ] **Step 3: Create portal middleware**

  Create `apps/partner-portal/src/middleware.ts`:
  ```typescript
  import { NextRequest, NextResponse } from "next/server";
  import { verifySession } from "@/lib/firebase/admin";

  const LANDING_URL = process.env.LANDING_URL ?? "http://localhost:3004";

  export async function middleware(req: NextRequest) {
    const token = req.cookies.get("__session")?.value;
    if (!token) return redirectToLogin(req);

    const session = await verifySession(token);
    if (!session) return redirectToLogin(req);

    if (session.role && session.role !== "partner") {
      return redirectToLogin(req, "unauthorized");
    }

    return NextResponse.next();
  }

  function redirectToLogin(req: NextRequest, error?: string) {
    const url = new URL(`${LANDING_URL}/login`);
    url.searchParams.set("role", "partner");
    if (error) url.searchParams.set("error", error);
    return NextResponse.redirect(url);
  }

  export const config = {
    matcher: ["/((?!_next/static|_next/image|favicon.ico).*)"],
  };
  ```

- [ ] **Step 4: Remove auth routes**

  ```bash
  rm -rf apps/partner-portal/src/app/\(auth\)/login
  rm -rf apps/partner-portal/src/app/\(auth\)/register
  rmdir apps/partner-portal/src/app/\(auth\) 2>/dev/null || true
  ```

- [ ] **Step 5: Commit**

  ```bash
  git add apps/partner-portal/next.config.mjs \
          apps/partner-portal/src/lib/firebase/ \
          apps/partner-portal/src/middleware.ts
  git rm -r apps/partner-portal/src/app/\(auth\)/ 2>/dev/null || true
  git commit -m "feat(partner-portal): add basePath, Firebase middleware, remove local auth"
  ```

---

## Task 13: Portal basePath + firebase-admin (customer-portal)

**Files:**
- Modify: `apps/customer-portal/next.config.mjs`
- Create: `apps/customer-portal/src/lib/firebase/admin.ts`
- Create: `apps/customer-portal/src/middleware.ts`
- Delete: `apps/customer-portal/src/app/(auth)/login/`
- Delete: `apps/customer-portal/src/app/(auth)/register/`

- [ ] **Step 1: Update next.config.mjs**

  Replace contents of `apps/customer-portal/next.config.mjs`:
  ```javascript
  /** @type {import('next').NextConfig} */
  const nextConfig = {
    output: "standalone",
    basePath: "/customer",
    serverExternalPackages: ["firebase-admin"],
    images: {
      remotePatterns: [
        { protocol: "https", hostname: "**.amazonaws.com" },
      ],
    },
    typescript: { ignoreBuildErrors: true },
    eslint:     { ignoreDuringBuilds: true },
  };

  export default nextConfig;
  ```

- [ ] **Step 2: Create Firebase Admin helper**

  Create `apps/customer-portal/src/lib/firebase/admin.ts`:
  ```typescript
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
    uid:   string;
    email: string | undefined;
    role:  string | undefined;
  }

  export async function verifySession(token: string): Promise<SessionPayload | null> {
    try {
      const decoded = await getAuth(getAdminApp()).verifyIdToken(token);
      return { uid: decoded.uid, email: decoded.email, role: decoded.role as string | undefined };
    } catch {
      return null;
    }
  }
  ```

- [ ] **Step 3: Create portal middleware**

  Create `apps/customer-portal/src/middleware.ts`:
  ```typescript
  import { NextRequest, NextResponse } from "next/server";
  import { verifySession } from "@/lib/firebase/admin";

  const LANDING_URL = process.env.LANDING_URL ?? "http://localhost:3004";

  export async function middleware(req: NextRequest) {
    const token = req.cookies.get("__session")?.value;
    if (!token) return redirectToLogin(req);

    const session = await verifySession(token);
    if (!session) return redirectToLogin(req);

    if (session.role && session.role !== "customer") {
      return redirectToLogin(req, "unauthorized");
    }

    return NextResponse.next();
  }

  function redirectToLogin(req: NextRequest, error?: string) {
    const url = new URL(`${LANDING_URL}/login`);
    url.searchParams.set("role", "customer");
    if (error) url.searchParams.set("error", error);
    return NextResponse.redirect(url);
  }

  export const config = {
    matcher: ["/((?!_next/static|_next/image|favicon.ico).*)"],
  };
  ```

- [ ] **Step 4: Remove auth routes**

  ```bash
  rm -rf apps/customer-portal/src/app/\(auth\)/login
  rm -rf apps/customer-portal/src/app/\(auth\)/register
  rmdir apps/customer-portal/src/app/\(auth\) 2>/dev/null || true
  ```

- [ ] **Step 5: Commit**

  ```bash
  git add apps/customer-portal/next.config.mjs \
          apps/customer-portal/src/lib/firebase/ \
          apps/customer-portal/src/middleware.ts
  git rm -r apps/customer-portal/src/app/\(auth\)/ 2>/dev/null || true
  git commit -m "feat(customer-portal): add basePath, Firebase middleware, remove local auth"
  ```

---

## Task 14: Dokploy — redeploy landing as primary domain

- [ ] **Step 1: Push all changes to remote**

  ```bash
  git push origin master
  ```

- [ ] **Step 2: In Dokploy — update landing app domain**

  - Go to **LogisticOS org** → **landing app** → **Domains** tab
  - Set domain to `os.cargomarket.net`, port `3004`
  - Redeploy

- [ ] **Step 3: In Dokploy — update merchant portal**

  - Go to **merchant-portal app** → **Domains** tab
  - **Remove** the `os.cargomarket.net` domain entry (it moves to landing)
  - Leave merchant-portal with **no public domain** — it will be accessed only via landing rewrite
  - Redeploy merchant-portal

- [ ] **Step 4: In Dokploy — deploy admin, partner, customer portals**

  For each portal, create a new Dokploy app with:
  - **Build Type**: Dockerfile
  - **Dockerfile Path**: `apps/<portal>/Dockerfile`
  - **Build Context**: `.`
  - **No public domain** (internal only)
  - Redeploy

- [ ] **Step 5: Set env vars in Dokploy for each app**

  For the **landing app**, set these env vars in the Dokploy environment tab:
  ```
  NEXT_PUBLIC_FIREBASE_API_KEY=<value>
  NEXT_PUBLIC_FIREBASE_AUTH_DOMAIN=<value>
  NEXT_PUBLIC_FIREBASE_PROJECT_ID=<value>
  NEXT_PUBLIC_FIREBASE_APP_ID=<value>
  FIREBASE_SERVICE_ACCOUNT_JSON=<base64-value>
  MERCHANT_PORTAL_URL=http://<merchant-container-name>:3000
  ADMIN_PORTAL_URL=http://<admin-container-name>:3001
  PARTNER_PORTAL_URL=http://<partner-container-name>:3003
  CUSTOMER_PORTAL_URL=http://<customer-container-name>:3002
  ```

  For each **portal app**:
  ```
  NEXT_PUBLIC_FIREBASE_API_KEY=<value>
  NEXT_PUBLIC_FIREBASE_AUTH_DOMAIN=<value>
  NEXT_PUBLIC_FIREBASE_PROJECT_ID=<value>
  NEXT_PUBLIC_FIREBASE_APP_ID=<value>
  FIREBASE_SERVICE_ACCOUNT_JSON=<base64-value>
  LANDING_URL=https://os.cargomarket.net
  ```

  > **Note:** Container names in Dokploy follow the pattern `<projectname>-<appname>-<randomid>`. Find the exact names by running `docker ps --format "{{.Names}}"` on the server after deployment.

- [ ] **Step 6: Set Firebase custom claims for admin users**

  In Firebase Console → **Authentication** → find your admin user → or use the Admin SDK script:

  ```javascript
  // Run once: node scripts/set-admin-claim.js
  const admin = require("firebase-admin");
  const serviceAccount = require("./service-account.json");
  admin.initializeApp({ credential: admin.credential.cert(serviceAccount) });
  admin.auth().setCustomUserClaims("<USER_UID>", { role: "admin" })
    .then(() => console.log("Done"))
    .catch(console.error);
  ```

- [ ] **Step 7: Smoke test**

  1. `https://os.cargomarket.net` — landing page loads
  2. `https://os.cargomarket.net/track` — tracking page loads, AWB search works
  3. `https://os.cargomarket.net/login` — login page loads with 4 role cards
  4. `https://os.cargomarket.net/merchant` — redirects to `/login?role=merchant` (not logged in)
  5. Sign in as merchant → lands on `/merchant` dashboard
  6. Sign in as wrong role → error message shown

---

## Task 15: Final push

- [ ] **Step 1: Push all commits**

  ```bash
  git push origin master
  ```

- [ ] **Step 2: Verify deployment**

  ```bash
  curl -I https://os.cargomarket.net
  curl -I https://os.cargomarket.net/track
  curl -I https://os.cargomarket.net/merchant   # should return 307 redirect to /login
  ```

  Expected:
  - `/` → `200 OK`
  - `/track` → `200 OK`
  - `/merchant` → `307 Temporary Redirect` to `/login?role=merchant`
