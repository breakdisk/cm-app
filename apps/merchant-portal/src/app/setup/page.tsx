"use client";

/**
 * Merchant onboarding — setup page for draft tenants.
 *
 * Called immediately after the Firebase → LoS JWT exchange when the tenant
 * is still in `draft` status. Collects the three fields required to call
 * POST /v1/tenants/me/finalize: business_name, currency (ISO 4217), and
 * region (ISO 3166-1 alpha-2).
 *
 * After finalization the refresh endpoint is called so the access-token JWT
 * is upgraded from the draft-scoped token to the full tenant_admin token.
 */

import { useState } from "react";
import { useRouter } from "next/navigation";
import { motion } from "framer-motion";
import { Zap, Building2, Globe, ChevronRight, Loader2, CheckCircle2 } from "lucide-react";
import { identityApi } from "@/lib/api/identity";

// ── Supported currencies + regions ───────────────────────────────────────────

const CURRENCIES = [
  { code: "PHP", label: "Philippine Peso (PHP)" },
  { code: "AED", label: "UAE Dirham (AED)" },
  { code: "USD", label: "US Dollar (USD)" },
  { code: "SGD", label: "Singapore Dollar (SGD)" },
  { code: "MYR", label: "Malaysian Ringgit (MYR)" },
  { code: "IDR", label: "Indonesian Rupiah (IDR)" },
  { code: "THB", label: "Thai Baht (THB)" },
  { code: "VND", label: "Vietnamese Dong (VND)" },
  { code: "SAR", label: "Saudi Riyal (SAR)" },
  { code: "QAR", label: "Qatari Riyal (QAR)" },
  { code: "KWD", label: "Kuwaiti Dinar (KWD)" },
  { code: "AUD", label: "Australian Dollar (AUD)" },
  { code: "EUR", label: "Euro (EUR)" },
  { code: "GBP", label: "British Pound (GBP)" },
];

const REGIONS = [
  { code: "PH", label: "Philippines (PH)" },
  { code: "AE", label: "United Arab Emirates (AE)" },
  { code: "SG", label: "Singapore (SG)" },
  { code: "MY", label: "Malaysia (MY)" },
  { code: "ID", label: "Indonesia (ID)" },
  { code: "TH", label: "Thailand (TH)" },
  { code: "VN", label: "Vietnam (VN)" },
  { code: "SA", label: "Saudi Arabia (SA)" },
  { code: "QA", label: "Qatar (QA)" },
  { code: "KW", label: "Kuwait (KW)" },
  { code: "AU", label: "Australia (AU)" },
  { code: "US", label: "United States (US)" },
  { code: "GB", label: "United Kingdom (GB)" },
  { code: "DE", label: "Germany (DE)" },
];

// ── Styles ────────────────────────────────────────────────────────────────────

const inputCls =
  "w-full rounded-lg border border-white/10 bg-white/5 px-3.5 py-3 text-sm text-white " +
  "placeholder:text-white/25 outline-none focus:border-cyan-neon/50 focus:ring-1 focus:ring-cyan-neon/20 " +
  "transition-all font-mono";

const labelCls = "block text-xs font-medium text-white/50 mb-2";

// ── Page ─────────────────────────────────────────────────────────────────────

export default function SetupPage() {
  const router = useRouter();

  const [businessName, setBusinessName] = useState("");
  const [currency, setCurrency]         = useState("PHP");
  const [region, setRegion]             = useState("PH");
  const [loading, setLoading]           = useState(false);
  const [done, setDone]                 = useState(false);
  const [error, setError]               = useState<string | null>(null);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!businessName.trim() || businessName.trim().length < 2) {
      setError("Business name must be at least 2 characters.");
      return;
    }
    setError(null);
    setLoading(true);
    try {
      await identityApi.finalizeTenant({
        business_name: businessName.trim(),
        currency,
        region,
      });

      // Upgrade the access-token JWT from draft-scoped to full tenant_admin.
      await fetch("/api/auth/refresh", { method: "POST", credentials: "same-origin" });

      setDone(true);
      setTimeout(() => router.replace("/"), 1200);
    } catch (err: unknown) {
      const e = err as { message?: string };
      setError(e.message ?? "Setup failed. Please try again.");
      setLoading(false);
    }
  }

  return (
    <div
      className="flex min-h-screen flex-col items-center justify-center p-6"
      style={{ background: "#050810" }}
    >
      {/* Glow backdrop */}
      <div
        className="pointer-events-none fixed inset-0"
        style={{
          background:
            "radial-gradient(ellipse 60% 40% at 50% 0%, rgba(0,229,255,0.08) 0%, transparent 70%)",
        }}
      />

      <motion.div
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.4, ease: [0.16, 1, 0.3, 1] }}
        className="relative w-full max-w-md"
      >
        {/* Logo */}
        <div className="mb-8 flex flex-col items-center gap-3">
          <div
            className="flex h-12 w-12 items-center justify-center rounded-2xl"
            style={{
              background: "linear-gradient(135deg, #00E5FF 0%, #A855F7 100%)",
              boxShadow: "0 0 24px rgba(0,229,255,0.4)",
            }}
          >
            <Zap className="h-6 w-6 text-white" strokeWidth={2.5} />
          </div>
          <div className="text-center">
            <h1 className="font-heading text-2xl font-bold text-white">
              Welcome to CargoMarket
            </h1>
            <p className="mt-1 text-sm text-white/40">
              Let&apos;s set up your merchant account
            </p>
          </div>
        </div>

        {/* Card */}
        <div
          className="rounded-2xl border border-white/8 p-6"
          style={{
            background: "rgba(255,255,255,0.04)",
            backdropFilter: "blur(20px)",
          }}
        >
          {done ? (
            <motion.div
              initial={{ opacity: 0, scale: 0.95 }}
              animate={{ opacity: 1, scale: 1 }}
              className="flex flex-col items-center gap-4 py-4 text-center"
            >
              <div
                className="flex h-14 w-14 items-center justify-center rounded-full"
                style={{ background: "rgba(0,255,136,0.1)", border: "1px solid rgba(0,255,136,0.3)" }}
              >
                <CheckCircle2 className="h-7 w-7 text-green-signal" />
              </div>
              <div>
                <p className="font-heading text-lg font-bold text-white">
                  Account activated!
                </p>
                <p className="mt-1 text-sm text-white/40">Redirecting to your dashboard…</p>
              </div>
            </motion.div>
          ) : (
            <form onSubmit={handleSubmit} className="space-y-5">
              {/* Business name */}
              <div>
                <label className={labelCls}>
                  <Building2 className="mr-1.5 inline h-3 w-3" />
                  Business / Company Name
                </label>
                <input
                  className={inputCls}
                  value={businessName}
                  onChange={(e) => setBusinessName(e.target.value)}
                  placeholder="Atlas Cargo Solutions Inc."
                  maxLength={100}
                  required
                  autoFocus
                />
              </div>

              {/* Currency + Region row */}
              <div className="grid grid-cols-2 gap-3">
                <div>
                  <label className={labelCls}>
                    <Globe className="mr-1.5 inline h-3 w-3" />
                    Primary Currency
                  </label>
                  <select
                    className={inputCls}
                    value={currency}
                    onChange={(e) => setCurrency(e.target.value)}
                  >
                    {CURRENCIES.map((c) => (
                      <option key={c.code} value={c.code}>
                        {c.code} — {c.label.split(" (")[0]}
                      </option>
                    ))}
                  </select>
                </div>
                <div>
                  <label className={labelCls}>Region</label>
                  <select
                    className={inputCls}
                    value={region}
                    onChange={(e) => setRegion(e.target.value)}
                  >
                    {REGIONS.map((r) => (
                      <option key={r.code} value={r.code}>
                        {r.code} — {r.label.split(" (")[0]}
                      </option>
                    ))}
                  </select>
                </div>
              </div>

              {error && (
                <p className="rounded-lg border border-red-500/20 bg-red-500/10 px-3 py-2 text-xs text-red-400">
                  {error}
                </p>
              )}

              <button
                type="submit"
                disabled={loading}
                className="flex w-full items-center justify-center gap-2 rounded-xl py-3 text-sm font-semibold text-white transition-all disabled:opacity-50"
                style={{
                  background: "linear-gradient(135deg, #00E5FF20, #A855F720)",
                  border: "1px solid rgba(0,229,255,0.3)",
                }}
              >
                {loading ? (
                  <Loader2 className="h-4 w-4 animate-spin" />
                ) : (
                  <ChevronRight className="h-4 w-4" />
                )}
                {loading ? "Activating account…" : "Activate Merchant Account"}
              </button>
            </form>
          )}
        </div>

        <p className="mt-4 text-center text-xs text-white/25">
          You can update these details later from Settings
        </p>
      </motion.div>
    </div>
  );
}
