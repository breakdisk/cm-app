"use client";

import { Suspense, useState } from "react";
import { useSearchParams, useRouter } from "next/navigation";
import { motion } from "framer-motion";
import { CheckCircle2, XCircle, Loader2, Zap, Eye, EyeOff, Lock } from "lucide-react";

const API_BASE = process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:8000";

type Step = "form" | "success" | "no-token";

function ResetPasswordInner() {
  const searchParams             = useSearchParams();
  const router                   = useRouter();
  const token                    = searchParams.get("token");

  const [step, setStep]          = useState<Step>(token ? "form" : "no-token");
  const [password, setPassword]  = useState("");
  const [confirm, setConfirm]    = useState("");
  const [showPw, setShowPw]      = useState(false);
  const [showCf, setShowCf]      = useState(false);
  const [loading, setLoading]    = useState(false);
  const [error, setError]        = useState<string | null>(null);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!token) return;
    setError(null);

    if (password.length < 8) {
      setError("Password must be at least 8 characters.");
      return;
    }
    if (password !== confirm) {
      setError("Passwords do not match.");
      return;
    }

    setLoading(true);
    try {
      const res = await fetch(`${API_BASE}/v1/auth/reset-password`, {
        method:  "POST",
        headers: { "Content-Type": "application/json" },
        body:    JSON.stringify({ token, new_password: password }),
        cache:   "no-store",
      });
      if (res.ok) {
        setStep("success");
      } else {
        const body = await res.json().catch(() => ({})) as { error?: { message?: string } };
        setError(
          body?.error?.message ??
          "The reset link is invalid or has expired. Please request a new one."
        );
      }
    } catch {
      setError("Unable to reach the server. Please try again.");
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="min-h-screen bg-[#050810] flex items-center justify-center px-4">
      <div
        className="pointer-events-none fixed inset-0 opacity-30"
        style={{
          backgroundImage:
            "linear-gradient(rgba(0,229,255,0.04) 1px, transparent 1px), linear-gradient(90deg, rgba(0,229,255,0.04) 1px, transparent 1px)",
          backgroundSize: "48px 48px",
        }}
      />

      <motion.div
        initial={{ opacity: 0, y: 24 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.5, ease: [0.16, 1, 0.3, 1] }}
        className="w-full max-w-md"
      >
        <a href="/" className="flex items-center gap-2.5 justify-center mb-8">
          <div className="relative w-8 h-8 flex items-center justify-center">
            <div className="absolute inset-0 rounded-lg bg-gradient-to-br from-cyan-500/30 to-purple-500/30" />
            <Zap className="w-4 h-4 text-cyan-400 relative z-10" strokeWidth={2.5} />
          </div>
          <span className="text-lg font-bold tracking-tight">
            <span
              className="bg-gradient-to-r from-cyan-400 via-purple-400 to-green-400 bg-clip-text text-transparent"
            >
              Cargo
            </span>
            <span className="text-white">Market</span>
          </span>
        </a>

        <div className="rounded-2xl border border-white/[0.08] bg-white/[0.03] backdrop-blur-xl p-8 shadow-2xl">
          {step === "form" && (
            <>
              <div className="w-12 h-12 rounded-xl bg-cyan-500/10 border border-cyan-500/20 flex items-center justify-center mb-5">
                <Lock className="w-6 h-6 text-cyan-400" />
              </div>
              <h1 className="text-xl font-bold text-white mb-1">Set new password</h1>
              <p className="text-sm text-white/40 mb-6">
                Enter a new password for your account. Must be at least 8 characters.
              </p>

              {error && (
                <div className="mb-4 rounded-xl border border-red-500/20 bg-red-500/10 px-4 py-3 text-sm text-red-400 flex items-start gap-2">
                  <XCircle className="w-4 h-4 mt-0.5 shrink-0" />
                  {error}
                </div>
              )}

              <form onSubmit={handleSubmit} className="flex flex-col gap-4">
                <div className="relative">
                  <input
                    type={showPw ? "text" : "password"}
                    value={password}
                    onChange={(e) => setPassword(e.target.value)}
                    placeholder="New password"
                    required
                    minLength={8}
                    className="w-full rounded-xl border border-white/[0.08] bg-white/[0.04] px-4 py-3 pr-11 text-sm text-white placeholder:text-white/20 outline-none focus:border-cyan-400/40 transition-colors"
                  />
                  <button
                    type="button"
                    onClick={() => setShowPw((v) => !v)}
                    className="absolute right-3 top-1/2 -translate-y-1/2 text-white/30 hover:text-white/60 transition-colors"
                    aria-label={showPw ? "Hide password" : "Show password"}
                  >
                    {showPw ? <EyeOff className="w-4 h-4" /> : <Eye className="w-4 h-4" />}
                  </button>
                </div>

                <div className="relative">
                  <input
                    type={showCf ? "text" : "password"}
                    value={confirm}
                    onChange={(e) => setConfirm(e.target.value)}
                    placeholder="Confirm new password"
                    required
                    className="w-full rounded-xl border border-white/[0.08] bg-white/[0.04] px-4 py-3 pr-11 text-sm text-white placeholder:text-white/20 outline-none focus:border-cyan-400/40 transition-colors"
                  />
                  <button
                    type="button"
                    onClick={() => setShowCf((v) => !v)}
                    className="absolute right-3 top-1/2 -translate-y-1/2 text-white/30 hover:text-white/60 transition-colors"
                    aria-label={showCf ? "Hide password" : "Show password"}
                  >
                    {showCf ? <EyeOff className="w-4 h-4" /> : <Eye className="w-4 h-4" />}
                  </button>
                </div>

                <button
                  type="submit"
                  disabled={loading || !password || !confirm}
                  className="flex items-center justify-center gap-2 rounded-xl bg-cyan-400 px-4 py-3 text-sm font-semibold text-[#050810] hover:shadow-[0_0_20px_rgba(0,229,255,0.4)] transition-all duration-200 disabled:opacity-50 disabled:cursor-not-allowed"
                >
                  {loading ? <Loader2 className="w-4 h-4 animate-spin" /> : "Reset password"}
                </button>
              </form>
            </>
          )}

          {step === "success" && (
            <div className="text-center">
              <div className="w-14 h-14 rounded-full bg-green-500/10 border border-green-500/20 flex items-center justify-center mx-auto mb-4">
                <CheckCircle2 className="w-7 h-7 text-green-400" />
              </div>
              <h1 className="text-xl font-bold text-white mb-2">Password updated</h1>
              <p className="text-sm text-white/50 mb-6">
                Your password has been reset. Sign in with your new credentials.
              </p>
              <button
                onClick={() => router.push("/login")}
                className="inline-flex items-center justify-center gap-2 rounded-xl bg-cyan-400 px-6 py-3 text-sm font-semibold text-[#050810] hover:shadow-[0_0_20px_rgba(0,229,255,0.4)] transition-all duration-200"
              >
                Sign in
              </button>
            </div>
          )}

          {step === "no-token" && (
            <div className="text-center">
              <div className="w-14 h-14 rounded-full bg-amber-500/10 border border-amber-500/20 flex items-center justify-center mx-auto mb-4">
                <XCircle className="w-7 h-7 text-amber-400" />
              </div>
              <h1 className="text-xl font-bold text-white mb-2">Invalid link</h1>
              <p className="text-sm text-white/50 mb-6">
                This reset link is missing or malformed. Request a new password reset from the sign-in page.
              </p>
              <a
                href="/login"
                className="inline-flex items-center justify-center gap-2 rounded-xl border border-white/[0.08] bg-white/[0.04] px-6 py-3 text-sm font-medium text-white/70 hover:bg-white/[0.08] hover:text-white transition-all duration-200"
              >
                Back to sign in
              </a>
            </div>
          )}
        </div>
      </motion.div>
    </div>
  );
}

export default function ResetPasswordPage() {
  return (
    <Suspense>
      <ResetPasswordInner />
    </Suspense>
  );
}
