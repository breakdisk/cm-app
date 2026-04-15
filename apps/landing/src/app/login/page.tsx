"use client";

import { Suspense, useEffect, useState } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import { motion, AnimatePresence } from "framer-motion";
import { getFirebaseApp } from "@/lib/firebase/client";
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

function LoginPageInner() {
  const router       = useRouter();
  const searchParams = useSearchParams();
  const [selectedRole, setSelectedRole] = useState<RoleId | null>(
    (searchParams.get("role") as RoleId) ?? null
  );
  const [loading, setLoading]     = useState(false);
  const [email, setEmail]         = useState("");
  const [emailSent, setEmailSent] = useState(false);
  const errorParam = searchParams.get("error");
  const [error, setError] = useState<string | null>(
    errorParam === "unauthorized" ? "You don't have access to that portal." :
    errorParam === "expired"      ? "Your session expired. Please sign in again." :
    null
  );
  const [showMagic, setShowMagic] = useState(false);

  const auth = getAuth(getFirebaseApp());

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
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function completeSignIn(idToken: string) {
    const role = selectedRole ?? (searchParams.get("role") as RoleId) ?? "customer";
    const res  = await fetch("/api/auth/session", {
      method:  "POST",
      headers: { "Content-Type": "application/json" },
      body:    JSON.stringify({ idToken, role }),
    });
    if (!res.ok) {
      const body = await res.json().catch(() => ({}));
      setError((body as { error?: string }).error ?? "Sign-in failed.");
      return;
    }
    // Draft merchants are redirected to the onboarding wizard; everyone else
    // lands on their portal home.
    const body = (await res.json().catch(() => ({}))) as { onboarding_required?: boolean };
    if (body.onboarding_required) {
      router.push(`/setup?role=${role}`);
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
                  <button
                    onClick={signInWithGoogle}
                    disabled={loading}
                    className="flex items-center justify-center gap-3 rounded-xl border border-white/[0.08] bg-white/[0.04] px-4 py-3 text-sm font-medium text-white/70 hover:bg-white/[0.08] hover:text-white transition-all duration-200 disabled:opacity-50"
                  >
                    {loading ? <Loader2 className="h-4 w-4 animate-spin" /> : <GoogleIcon />}
                    Continue with Google
                  </button>

                  <button
                    onClick={signInWithFacebook}
                    disabled={loading}
                    className="flex items-center justify-center gap-3 rounded-xl border border-white/[0.08] bg-white/[0.04] px-4 py-3 text-sm font-medium text-white/70 hover:bg-white/[0.08] hover:text-white transition-all duration-200 disabled:opacity-50"
                  >
                    {loading ? <Loader2 className="h-4 w-4 animate-spin" /> : <Mail className="h-4 w-4" />}
                    Continue with Facebook
                  </button>

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

export default function LoginPage() {
  return (
    <Suspense>
      <LoginPageInner />
    </Suspense>
  );
}
