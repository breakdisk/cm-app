"use client";

import { useState, type FormEvent } from "react";
import { useRouter } from "next/navigation";
import { motion } from "framer-motion";
import { Eye, EyeOff, LogIn, Loader2, Zap, ShieldCheck } from "lucide-react";
import { cn } from "@/lib/design-system/cn";

// ─── Portal config ────────────────────────────────────────────────────────────

const PORTAL_NAME  = "Ops Console";
const ACCENT_COLOR = "#A855F7";
const ACCENT_DIM   = "rgba(168, 85, 247, 0.20)";
const ACCENT_RING  = "rgba(168, 85, 247, 0.35)";
const GRADIENT     = "linear-gradient(135deg, #A855F7 0%, #00E5FF 100%)";

// ─── Aurora with purple tint ──────────────────────────────────────────────────

function AdminAuroraBackground({ children }: { children: React.ReactNode }) {
  return (
    <div
      className="relative flex min-h-screen flex-col overflow-hidden"
      style={{ background: "#050810" }}
    >
      {/* Aurora blobs — purple-dominant */}
      <div className="pointer-events-none absolute inset-0 overflow-hidden">
        <div
          className="absolute -top-40 -left-40 h-[600px] w-[600px] rounded-full opacity-20 blur-[120px] animate-aurora"
          style={{ background: "linear-gradient(135deg, #A855F7, #00E5FF)" }}
        />
        <div
          className="absolute -bottom-40 -right-20 h-[500px] w-[500px] rounded-full opacity-15 blur-[100px] animate-aurora"
          style={{
            background: "linear-gradient(135deg, #7C3AED, #A855F7)",
            animationDelay: "-4s",
          }}
        />
        <div
          className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 h-[400px] w-[400px] rounded-full opacity-10 blur-[90px] animate-aurora"
          style={{
            background: "linear-gradient(135deg, #00E5FF, #A855F7)",
            animationDelay: "-2s",
          }}
        />
      </div>
      {/* Grid overlay */}
      <div
        className="pointer-events-none absolute inset-0 opacity-20"
        style={{
          backgroundImage:
            "linear-gradient(rgba(168,85,247,0.05) 1px, transparent 1px), linear-gradient(90deg, rgba(168,85,247,0.05) 1px, transparent 1px)",
          backgroundSize: "48px 48px",
        }}
      />
      <div className="relative z-10 flex flex-1 flex-col">{children}</div>
    </div>
  );
}

// ─── Google icon ──────────────────────────────────────────────────────────────

function GoogleIcon() {
  return (
    <svg viewBox="0 0 24 24" className="h-4 w-4" aria-hidden="true">
      <path d="M22.56 12.25c0-.78-.07-1.53-.2-2.25H12v4.26h5.92c-.26 1.37-1.04 2.53-2.21 3.31v2.77h3.57c2.08-1.92 3.28-4.74 3.28-8.09z" fill="#4285F4" />
      <path d="M12 23c2.97 0 5.46-.98 7.28-2.66l-3.57-2.77c-.98.66-2.23 1.06-3.71 1.06-2.86 0-5.29-1.93-6.16-4.53H2.18v2.84C3.99 20.53 7.7 23 12 23z" fill="#34A853" />
      <path d="M5.84 14.09c-.22-.66-.35-1.36-.35-2.09s.13-1.43.35-2.09V7.07H2.18C1.43 8.55 1 10.22 1 12s.43 3.45 1.18 4.93l2.85-2.22.81-.62z" fill="#FBBC05" />
      <path d="M12 5.38c1.62 0 3.06.56 4.21 1.64l3.15-3.15C17.45 2.09 14.97 1 12 1 7.7 1 3.99 3.47 2.18 7.07l3.66 2.84c.87-2.6 3.3-4.53 6.16-4.53z" fill="#EA4335" />
    </svg>
  );
}

// ─── Input ────────────────────────────────────────────────────────────────────

function AuthInput({
  label,
  id,
  type,
  value,
  onChange,
  placeholder,
  autoComplete,
  required,
  rightElement,
}: {
  label: string;
  id: string;
  type: string;
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  autoComplete?: string;
  required?: boolean;
  rightElement?: React.ReactNode;
}) {
  const [focused, setFocused] = useState(false);

  return (
    <div className="space-y-1.5">
      <label htmlFor={id} className="block text-xs font-medium text-white/60">
        {label}
      </label>
      <div className="relative">
        <input
          id={id}
          type={type}
          value={value}
          onChange={(e) => onChange(e.target.value)}
          placeholder={placeholder}
          autoComplete={autoComplete}
          required={required}
          onFocus={() => setFocused(true)}
          onBlur={() => setFocused(false)}
          className={cn(
            "w-full rounded-xl border bg-glass-100 px-4 py-3 text-sm text-white placeholder-white/20",
            "outline-none transition-all duration-200",
            rightElement && "pr-11"
          )}
          style={{
            borderColor: focused ? ACCENT_COLOR : "rgba(255,255,255,0.08)",
            boxShadow: focused
              ? `0 0 0 3px ${ACCENT_RING}, inset 0 1px 0 rgba(255,255,255,0.04)`
              : "inset 0 1px 0 rgba(255,255,255,0.04)",
          }}
        />
        {rightElement && (
          <div className="absolute inset-y-0 right-0 flex items-center pr-3">
            {rightElement}
          </div>
        )}
      </div>
    </div>
  );
}

// ─── Page ─────────────────────────────────────────────────────────────────────

export default function AdminLoginPage() {
  const router = useRouter();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [showPassword, setShowPassword] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    setError(null);
    setLoading(true);

    await new Promise((r) => setTimeout(r, 1400));

    if (email && password) {
      router.push("/dispatch");
    } else {
      setError("Invalid credentials. Admin access is restricted.");
      setLoading(false);
    }
  }

  return (
    <AdminAuroraBackground>
      <div className="flex flex-1 items-center justify-center px-4 py-16">
        <motion.div
          initial={{ opacity: 0, y: 24 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.5, ease: [0.16, 1, 0.3, 1] }}
          className="w-full max-w-md"
        >
          <div
            className="rounded-2xl p-8"
            style={{
              background: "rgba(13, 20, 34, 0.75)",
              backdropFilter: "blur(24px)",
              WebkitBackdropFilter: "blur(24px)",
              border: "1px solid rgba(168,85,247,0.15)",
              boxShadow:
                "0 8px 40px rgba(0,0,0,0.5), inset 0 1px 0 rgba(168,85,247,0.08)",
            }}
          >
            {/* Logo */}
            <div className="mb-8 flex flex-col items-center gap-3">
              <div
                className="flex h-12 w-12 items-center justify-center rounded-2xl"
                style={{
                  background: GRADIENT,
                  boxShadow: `0 0 28px ${ACCENT_DIM}`,
                }}
              >
                <Zap className="h-6 w-6 text-white" strokeWidth={2.5} />
              </div>
              <div className="text-center">
                <h1 className="font-heading text-xl font-bold text-white">
                  LogisticOS
                </h1>
                <p
                  className="mt-0.5 text-xs font-semibold uppercase tracking-[0.2em] font-mono"
                  style={{ color: ACCENT_COLOR, textShadow: `0 0 10px ${ACCENT_DIM}` }}
                >
                  {PORTAL_NAME}
                </p>
              </div>
              <div className="flex items-center gap-1.5 rounded-full border border-amber-signal/20 bg-amber-surface px-3 py-1">
                <ShieldCheck className="h-3 w-3 text-amber-signal" />
                <span className="text-2xs font-medium text-amber-signal font-mono uppercase tracking-wider">
                  Restricted Access
                </span>
              </div>
            </div>

            {/* Error */}
            {error && (
              <motion.div
                initial={{ opacity: 0, y: -4 }}
                animate={{ opacity: 1, y: 0 }}
                className="mb-4 rounded-lg border border-red-signal/30 bg-red-surface px-4 py-3 text-sm text-red-signal"
              >
                {error}
              </motion.div>
            )}

            {/* Form */}
            <form onSubmit={handleSubmit} className="space-y-4">
              <AuthInput
                label="Admin Email"
                id="email"
                type="email"
                value={email}
                onChange={setEmail}
                placeholder="admin@logisticos.io"
                autoComplete="email"
                required
              />

              <AuthInput
                label="Password"
                id="password"
                type={showPassword ? "text" : "password"}
                value={password}
                onChange={setPassword}
                placeholder="••••••••"
                autoComplete="current-password"
                required
                rightElement={
                  <button
                    type="button"
                    onClick={() => setShowPassword((s) => !s)}
                    className="text-white/30 transition-colors hover:text-white/70"
                    aria-label={showPassword ? "Hide password" : "Show password"}
                  >
                    {showPassword ? (
                      <EyeOff className="h-4 w-4" />
                    ) : (
                      <Eye className="h-4 w-4" />
                    )}
                  </button>
                }
              />

              <div className="flex items-center justify-end">
                <button
                  type="button"
                  className="text-xs transition-colors"
                  style={{ color: ACCENT_COLOR }}
                >
                  Forgot password?
                </button>
              </div>

              {/* Submit */}
              <button
                type="submit"
                disabled={loading}
                className={cn(
                  "relative w-full overflow-hidden rounded-xl px-6 py-3",
                  "text-sm font-semibold text-white transition-all duration-200",
                  "flex items-center justify-center gap-2",
                  loading ? "cursor-not-allowed opacity-70" : "hover:opacity-90 active:scale-[0.98]"
                )}
                style={{
                  background: loading
                    ? "rgba(168,85,247,0.3)"
                    : GRADIENT,
                  boxShadow: loading
                    ? "none"
                    : `0 0 20px ${ACCENT_DIM}`,
                }}
              >
                {loading ? (
                  <>
                    <Loader2 className="h-4 w-4 animate-spin" />
                    Authenticating…
                  </>
                ) : (
                  <>
                    <LogIn className="h-4 w-4" />
                    Sign In
                  </>
                )}
              </button>
            </form>

            {/* Divider */}
            <div className="my-5 flex items-center gap-3">
              <div className="h-px flex-1 bg-glass-border" />
              <span className="text-xs text-white/30">or</span>
              <div className="h-px flex-1 bg-glass-border" />
            </div>

            {/* Google SSO */}
            <button
              type="button"
              className={cn(
                "flex w-full items-center justify-center gap-3 rounded-xl border px-4 py-3",
                "text-sm font-medium text-white/70 transition-all duration-200",
                "border-glass-border bg-glass-100 hover:bg-glass-200 hover:text-white"
              )}
            >
              <GoogleIcon />
              Continue with Google
            </button>
          </div>

          {/* Footer */}
          <p className="mt-6 text-center text-xs text-white/20">
            LogisticOS — Secure Multi-Tenant Platform
          </p>
        </motion.div>
      </div>
    </AdminAuroraBackground>
  );
}
