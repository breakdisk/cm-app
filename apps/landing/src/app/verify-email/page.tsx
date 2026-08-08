"use client";

import { Suspense, useEffect, useState } from "react";
import { useSearchParams } from "next/navigation";
import { motion } from "framer-motion";
import { CheckCircle2, XCircle, Loader2, Zap, Mail } from "lucide-react";

import { API_BASE } from "@/lib/api-base";

type Status = "verifying" | "success" | "invalid" | "no-token";

function VerifyEmailInner() {
  const searchParams          = useSearchParams();
  const token                 = searchParams.get("token");
  const [status, setStatus]   = useState<Status>(token ? "verifying" : "no-token");
  const [message, setMessage] = useState<string>("");

  useEffect(() => {
    if (!token) return;
    (async () => {
      try {
        const res = await fetch(`${API_BASE}/v1/auth/verify-email`, {
          method:  "POST",
          headers: { "Content-Type": "application/json" },
          body:    JSON.stringify({ token }),
          cache:   "no-store",
        });
        if (res.ok) {
          setStatus("success");
        } else {
          const body = await res.json().catch(() => ({})) as { data?: { message?: string } };
          setMessage(body?.data?.message ?? "The verification link is invalid or has expired.");
          setStatus("invalid");
        }
      } catch {
        setMessage("Unable to reach the server. Please try again.");
        setStatus("invalid");
      }
    })();
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

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

        <div className="rounded-2xl border border-white/[0.08] bg-white/[0.03] backdrop-blur-xl p-8 shadow-2xl text-center">
          {status === "verifying" && (
            <>
              <Loader2 className="w-12 h-12 text-cyan-400 animate-spin mx-auto mb-4" />
              <h1 className="text-xl font-bold text-white mb-2">Verifying your email…</h1>
              <p className="text-sm text-white/40">This should only take a moment.</p>
            </>
          )}

          {status === "success" && (
            <>
              <div className="w-14 h-14 rounded-full bg-green-500/10 border border-green-500/20 flex items-center justify-center mx-auto mb-4">
                <CheckCircle2 className="w-7 h-7 text-green-400" />
              </div>
              <h1 className="text-xl font-bold text-white mb-2">Email verified</h1>
              <p className="text-sm text-white/50 mb-6">
                Your email address has been confirmed. You can now sign in to your account.
              </p>
              <a
                href="/login"
                className="inline-flex items-center justify-center gap-2 rounded-xl bg-cyan-400 px-6 py-3 text-sm font-semibold text-[#050810] hover:shadow-[0_0_20px_rgba(0,229,255,0.4)] transition-all duration-200"
              >
                Sign in
              </a>
            </>
          )}

          {status === "invalid" && (
            <>
              <div className="w-14 h-14 rounded-full bg-red-500/10 border border-red-500/20 flex items-center justify-center mx-auto mb-4">
                <XCircle className="w-7 h-7 text-red-400" />
              </div>
              <h1 className="text-xl font-bold text-white mb-2">Verification failed</h1>
              <p className="text-sm text-white/50 mb-6">
                {message || "The verification link is invalid or has expired."}
              </p>
              <a
                href="/login"
                className="inline-flex items-center justify-center gap-2 rounded-xl border border-white/[0.08] bg-white/[0.04] px-6 py-3 text-sm font-medium text-white/70 hover:bg-white/[0.08] hover:text-white transition-all duration-200"
              >
                Back to sign in
              </a>
            </>
          )}

          {status === "no-token" && (
            <>
              <div className="w-14 h-14 rounded-full bg-amber-500/10 border border-amber-500/20 flex items-center justify-center mx-auto mb-4">
                <Mail className="w-7 h-7 text-amber-400" />
              </div>
              <h1 className="text-xl font-bold text-white mb-2">Check your email</h1>
              <p className="text-sm text-white/50 mb-6">
                Click the verification link we sent to your email address to confirm your account.
              </p>
              <a
                href="/login"
                className="inline-flex items-center justify-center gap-2 rounded-xl border border-white/[0.08] bg-white/[0.04] px-6 py-3 text-sm font-medium text-white/70 hover:bg-white/[0.08] hover:text-white transition-all duration-200"
              >
                Back to sign in
              </a>
            </>
          )}
        </div>
      </motion.div>
    </div>
  );
}

export default function VerifyEmailPage() {
  return (
    <Suspense>
      <VerifyEmailInner />
    </Suspense>
  );
}
