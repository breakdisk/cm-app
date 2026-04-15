"use client";

import { Suspense, useState } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import { motion } from "framer-motion";
import { Building2, Globe2, Coins, Loader2, Zap, ArrowRight } from "lucide-react";

/**
 * Draft-tenant onboarding wizard.
 *
 * Reached from the login flow when identity returns `onboarding_required=true`.
 * The user arrives with a draft JWT whose only mutating permission is
 * `tenants:update-self`, so this page's single POST to `/api/tenants/finalize`
 * is the only write it can perform. On success the server rewrites the LoS
 * cookies with a non-draft JWT and we redirect into the portal.
 */

const CURRENCIES = [
  { code: "AED", label: "AED · UAE Dirham" },
  { code: "PHP", label: "PHP · Philippine Peso" },
  { code: "USD", label: "USD · US Dollar" },
  { code: "SGD", label: "SGD · Singapore Dollar" },
  { code: "EUR", label: "EUR · Euro" },
  { code: "GBP", label: "GBP · Pound Sterling" },
] as const;

const REGIONS = [
  { code: "AE", label: "United Arab Emirates" },
  { code: "PH", label: "Philippines" },
  { code: "SG", label: "Singapore" },
  { code: "US", label: "United States" },
  { code: "GB", label: "United Kingdom" },
  { code: "OTHER", label: "Other" },
] as const;

type PortalRole = "merchant" | "admin" | "partner" | "customer";

function SetupPageInner() {
  const router       = useRouter();
  const searchParams = useSearchParams();
  const role         = ((searchParams.get("role") as PortalRole | null) ?? "merchant");

  const [businessName, setBusinessName] = useState("");
  const [currency,     setCurrency]     = useState<string>("AED");
  const [region,       setRegion]       = useState<string>("AE");
  const [loading,      setLoading]      = useState(false);
  const [error,        setError]        = useState<string | null>(null);

  async function submit(event: React.FormEvent) {
    event.preventDefault();
    if (!businessName.trim() || businessName.trim().length < 2) {
      setError("Please enter your business name.");
      return;
    }
    if (region === "OTHER") {
      setError("Only AE, PH, SG, US, GB are supported at launch.");
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const res = await fetch("/api/tenants/finalize", {
        method:  "POST",
        headers: { "Content-Type": "application/json" },
        body:    JSON.stringify({
          business_name: businessName.trim(),
          currency,
          region,
        }),
      });
      if (!res.ok) {
        const body = (await res.json().catch(() => ({}))) as { error?: string };
        setError(body.error ?? "Setup failed. Please try again.");
        return;
      }
      router.push(`/${role}`);
    } catch {
      setError("Network error. Please try again.");
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
        <div className="flex items-center gap-2.5 justify-center mb-8">
          <div className="relative w-8 h-8 flex items-center justify-center">
            <div className="absolute inset-0 rounded-lg bg-gradient-to-br from-cyan-neon/30 to-purple-plasma/30" />
            <Zap className="w-4 h-4 text-cyan-neon relative z-10" strokeWidth={2.5} />
          </div>
          <span className="text-lg font-bold tracking-tight" style={{ fontFamily: "'Space Grotesk', sans-serif" }}>
            <span className="bg-gradient-to-r from-cyan-neon via-purple-plasma to-green-signal bg-clip-text text-transparent">Cargo</span>
            <span className="text-white">Market</span>
          </span>
        </div>

        <form
          onSubmit={submit}
          className="rounded-2xl border border-white/[0.08] bg-white/[0.03] backdrop-blur-xl p-8 shadow-glass-lg"
        >
          <h1 className="text-2xl font-bold text-white mb-1" style={{ fontFamily: "'Space Grotesk', sans-serif" }}>
            Finish setting up
          </h1>
          <p className="text-sm text-white/40 mb-6">
            Tell us a little about your business — you can change this later.
          </p>

          {error && (
            <div className="mb-4 rounded-xl border border-red-500/20 bg-red-500/10 px-4 py-3 text-sm text-red-400">
              {error}
            </div>
          )}

          <label className="block mb-4">
            <span className="flex items-center gap-2 text-xs font-medium text-white/60 mb-1.5">
              <Building2 className="h-3.5 w-3.5 text-cyan-neon" /> Business name
            </span>
            <input
              type="text"
              value={businessName}
              onChange={(e) => setBusinessName(e.target.value)}
              placeholder="Cargo Market PH"
              maxLength={100}
              className="w-full rounded-xl border border-white/[0.08] bg-white/[0.04] px-4 py-3 text-sm text-white placeholder:text-white/20 outline-none focus:border-cyan-neon/40"
              disabled={loading}
              required
            />
          </label>

          <div className="grid grid-cols-2 gap-3 mb-6">
            <label className="block">
              <span className="flex items-center gap-2 text-xs font-medium text-white/60 mb-1.5">
                <Coins className="h-3.5 w-3.5 text-amber-signal" /> Currency
              </span>
              <select
                value={currency}
                onChange={(e) => setCurrency(e.target.value)}
                disabled={loading}
                className="w-full rounded-xl border border-white/[0.08] bg-white/[0.04] px-3 py-3 text-sm text-white outline-none focus:border-cyan-neon/40"
              >
                {CURRENCIES.map((c) => (
                  <option key={c.code} value={c.code} className="bg-[#050810]">{c.label}</option>
                ))}
              </select>
            </label>

            <label className="block">
              <span className="flex items-center gap-2 text-xs font-medium text-white/60 mb-1.5">
                <Globe2 className="h-3.5 w-3.5 text-green-signal" /> Region
              </span>
              <select
                value={region}
                onChange={(e) => setRegion(e.target.value)}
                disabled={loading}
                className="w-full rounded-xl border border-white/[0.08] bg-white/[0.04] px-3 py-3 text-sm text-white outline-none focus:border-cyan-neon/40"
              >
                {REGIONS.map((r) => (
                  <option key={r.code} value={r.code} className="bg-[#050810]">{r.label}</option>
                ))}
              </select>
            </label>
          </div>

          <button
            type="submit"
            disabled={loading}
            className="flex w-full items-center justify-center gap-2 rounded-xl bg-cyan-neon px-4 py-3 text-sm font-semibold text-[#050810] hover:shadow-glow-cyan transition-all duration-200 disabled:opacity-50"
          >
            {loading ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : (
              <>
                Continue to {role} portal <ArrowRight className="h-4 w-4" />
              </>
            )}
          </button>

          <p className="mt-4 text-center text-xs text-white/30">
            Need to sign out?{" "}
            <a href="/api/auth/signout" className="text-white/60 hover:text-white">
              Start over
            </a>
          </p>
        </form>
      </motion.div>
    </div>
  );
}

export default function SetupPage() {
  return (
    <Suspense>
      <SetupPageInner />
    </Suspense>
  );
}
