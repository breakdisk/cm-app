"use client";

import { useState, type FormEvent } from "react";
import { useRouter } from "next/navigation";
import { motion } from "framer-motion";
import { Eye, EyeOff, LogIn, Loader2, Zap } from "lucide-react";
import { AuroraBackground } from "@/components/ui/aurora-background";
import { MovingBorderButton } from "@/components/ui/moving-border-button";
import { cn } from "@/lib/design-system/cn";

// ─── Portal config ────────────────────────────────────────────────────────────

const PORTAL_NAME  = "Merchant Portal";
const ACCENT_COLOR = "#00E5FF";
const ACCENT_DIM   = "rgba(0, 229, 255, 0.15)";
const ACCENT_RING  = "rgba(0, 229, 255, 0.4)";
const GRADIENT     = "linear-gradient(135deg, #00E5FF 0%, #A855F7 100%)";

// ─── Google icon (inline SVG, no external dep) ────────────────────────────────

function GoogleIcon() {
  return (
    <svg viewBox="0 0 24 24" className="h-4 w-4" aria-hidden="true">
      <path
        d="M22.56 12.25c0-.78-.07-1.53-.2-2.25H12v4.26h5.92c-.26 1.37-1.04 2.53-2.21 3.31v2.77h3.57c2.08-1.92 3.28-4.74 3.28-8.09z"
        fill="#4285F4"
      />
      <path
        d="M12 23c2.97 0 5.46-.98 7.28-2.66l-3.57-2.77c-.98.66-2.23 1.06-3.71 1.06-2.86 0-5.29-1.93-6.16-4.53H2.18v2.84C3.99 20.53 7.7 23 12 23z"
        fill="#34A853"
      />
      <path
        d="M5.84 14.09c-.22-.66-.35-1.36-.35-2.09s.13-1.43.35-2.09V7.07H2.18C1.43 8.55 1 10.22 1 12s.43 3.45 1.18 4.93l2.85-2.22.81-.62z"
        fill="#FBBC05"
      />
      <path
        d="M12 5.38c1.62 0 3.06.56 4.21 1.64l3.15-3.15C17.45 2.09 14.97 1 12 1 7.7 1 3.99 3.47 2.18 7.07l3.66 2.84c.87-2.6 3.3-4.53 6.16-4.53z"
        fill="#EA4335"
      />
    </svg>
  );
}

// ─── Input ────────────────────────────────────────────────────────────────────

interface InputProps {
  label: string;
  id: string;
  type: string;
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  autoComplete?: string;
  required?: boolean;
  rightElement?: React.ReactNode;
  accentColor: string;
  accentRing: string;
}

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
  accentColor,
  accentRing,
}: InputProps) {
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
            borderColor: focused ? accentColor : "rgba(255,255,255,0.08)",
            boxShadow: focused
              ? `0 0 0 3px ${accentRing}, inset 0 1px 0 rgba(255,255,255,0.04)`
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

export default function MerchantLoginPage() {
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
    try {
      const IDENTITY_URL = process.env.NEXT_PUBLIC_IDENTITY_URL ?? "http://localhost:8001";
      const res = await fetch(`${IDENTITY_URL}/v1/auth/login`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ email, password, tenant_slug: "demo" }),
      });
      const json = await res.json();
      if (!res.ok || !json.data?.access_token) {
        throw new Error(json.error?.message ?? "Invalid credentials");
      }
      localStorage.setItem("access_token", json.data.access_token);
      router.push("/");
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : "Login failed");
      setLoading(false);
    }
  }

  return (
    <AuroraBackground>
      <div className="flex flex-1 items-center justify-center px-4 py-16">
        <motion.div
          initial={{ opacity: 0, y: 24 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.5, ease: [0.16, 1, 0.3, 1] }}
          className="w-full max-w-md"
        >
          {/* Glass card */}
          <div
            className="rounded-2xl p-8"
            style={{
              background: "rgba(13, 20, 34, 0.7)",
              backdropFilter: "blur(24px)",
              WebkitBackdropFilter: "blur(24px)",
              border: "1px solid rgba(255,255,255,0.08)",
              boxShadow:
                "0 8px 40px rgba(0,0,0,0.5), inset 0 1px 0 rgba(255,255,255,0.06)",
            }}
          >
            {/* Logo */}
            <div className="mb-8 flex flex-col items-center gap-3">
              <div
                className="flex h-12 w-12 items-center justify-center rounded-2xl"
                style={{
                  background: GRADIENT,
                  boxShadow: `0 0 24px ${ACCENT_DIM}`,
                }}
              >
                <Zap className="h-6 w-6 text-white" strokeWidth={2.5} />
              </div>
              <div className="text-center">
                <h1 className="font-heading text-xl font-bold text-white">
                  LogisticOS
                </h1>
                <p
                  className="mt-0.5 text-xs font-medium uppercase tracking-widest font-mono"
                  style={{ color: ACCENT_COLOR }}
                >
                  {PORTAL_NAME}
                </p>
              </div>
              <p className="text-center text-sm text-white/50">
                Sign in to manage your shipments and campaigns
              </p>
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
                label="Email address"
                id="email"
                type="email"
                value={email}
                onChange={setEmail}
                placeholder="you@merchant.com"
                autoComplete="email"
                required
                accentColor={ACCENT_COLOR}
                accentRing={ACCENT_RING}
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
                accentColor={ACCENT_COLOR}
                accentRing={ACCENT_RING}
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
                  className="text-xs transition-colors hover:text-white/80"
                  style={{ color: ACCENT_COLOR }}
                >
                  Forgot password?
                </button>
              </div>

              {/* Submit */}
              <MovingBorderButton
                size="lg"
                variant="cyan"
                disabled={loading}
                className="w-full"
                containerClassName="w-full"
              >
                {loading ? (
                  <>
                    <Loader2 className="h-4 w-4 animate-spin" />
                    Signing in…
                  </>
                ) : (
                  <>
                    <LogIn className="h-4 w-4" />
                    Sign In
                  </>
                )}
              </MovingBorderButton>
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
    </AuroraBackground>
  );
}
