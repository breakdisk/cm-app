"use client";
/**
 * Customer Portal — Delivery Feedback
 * Post-delivery NPS / CSAT: rate experience, report issues, leave a review.
 */
import { Suspense } from "react";
import { useState, useTransition } from "react";
import { useSearchParams } from "next/navigation";
import { motion, AnimatePresence } from "framer-motion";
import { Star, CheckCircle2, AlertTriangle, Package, Clock, UserCheck, XCircle } from "lucide-react";

const CYAN   = "#00E5FF";
const GREEN  = "#00FF88";
const PURPLE = "#A855F7";
const AMBER  = "#FFAB00";
const RED    = "#FF3B5C";

const API_BASE = process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:8000";

const ISSUE_OPTIONS = [
  { id: "late",     label: "Delivery was late"         },
  { id: "damaged",  label: "Package arrived damaged"   },
  { id: "attitude", label: "Courier was unprofessional"},
  { id: "wrong",    label: "Wrong item / wrong address" },
  { id: "noshow",   label: "Courier never showed up"   },
  { id: "other",    label: "Other issue"               },
];

const ASPECT_LABELS = [
  { key: "speed",    icon: Clock,      label: "Delivery Speed"   },
  { key: "handling", icon: Package,    label: "Package Handling" },
  { key: "courier",  icon: UserCheck,  label: "Courier Service"  },
];

function StarRating({ value, onChange, size = 28 }: { value: number; onChange: (n: number) => void; size?: number }) {
  const [hover, setHover] = useState(0);
  return (
    <div className="flex gap-1">
      {[1, 2, 3, 4, 5].map((n) => (
        <button
          key={n}
          onClick={() => onChange(n)}
          onMouseEnter={() => setHover(n)}
          onMouseLeave={() => setHover(0)}
          className="transition-transform hover:scale-110"
        >
          <Star
            size={size}
            fill={(hover || value) >= n ? AMBER : "transparent"}
            stroke={(hover || value) >= n ? AMBER : "rgba(255,255,255,0.2)"}
            strokeWidth={1.5}
          />
        </button>
      ))}
    </div>
  );
}

function FeedbackContent() {
  const searchParams = useSearchParams();
  const AWB = (searchParams.get("tn") ?? "").toUpperCase();

  const [step, setStep]             = useState<"rate" | "issues" | "done">("rate");
  const [overallRating, setOverall] = useState(0);
  const [aspects, setAspects]       = useState({ speed: 0, handling: 0, courier: 0 });
  const [issues, setIssues]         = useState<string[]>([]);
  const [comment, setComment]       = useState("");
  const [error, setError]           = useState<string | null>(null);
  const [isPending, startTransition] = useTransition();

  function toggleIssue(id: string) {
    setIssues((prev) => prev.includes(id) ? prev.filter((x) => x !== id) : [...prev, id]);
  }

  function handleSubmit() {
    if (!overallRating || !AWB) return;
    setError(null);
    startTransition(async () => {
      try {
        const res = await fetch(
          `${API_BASE}/v1/tracking/${encodeURIComponent(AWB)}/feedback`,
          {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({
              rating:  overallRating,
              comment: comment.trim() || undefined,
              tags:    issues.length ? issues : undefined,
            }),
          }
        );
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        setStep("done");
      } catch {
        setError("Couldn't submit feedback. Please try again or contact support.");
      }
    });
  }

  if (step === "done") {
    return (
      <div className="min-h-[70vh] flex items-center justify-center px-4">
        <motion.div
          initial={{ opacity: 0, scale: 0.9 }}
          animate={{ opacity: 1, scale: 1 }}
          transition={{ type: "spring", stiffness: 200, damping: 20 }}
          className="text-center max-w-sm"
        >
          <div
            className="w-20 h-20 rounded-full flex items-center justify-center mx-auto mb-6"
            style={{ background: `${GREEN}15`, border: `1px solid ${GREEN}40` }}
          >
            <CheckCircle2 size={36} color={GREEN} />
          </div>
          <h2 className="text-2xl font-bold text-white font-space-grotesk mb-3">Thank you!</h2>
          <p className="text-white/50 text-sm leading-relaxed">
            Your feedback for <span className="font-mono text-[#00E5FF]">{AWB}</span> has been submitted.
            It helps us improve our service for everyone.
          </p>
          {overallRating >= 4 && (
            <motion.div
              initial={{ opacity: 0, y: 10 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ delay: 0.4 }}
              className="mt-6 p-4 rounded-xl text-sm"
              style={{ background: `${PURPLE}15`, border: `1px solid ${PURPLE}30` }}
            >
              <p className="text-white font-medium mb-1">+25 Loyalty Points earned!</p>
              <p className="text-white/40 text-xs">Added to your LogisticOS loyalty account.</p>
            </motion.div>
          )}
          <a href="/" className="mt-6 inline-block text-sm text-[#00E5FF] hover:text-[#00E5FF]/70 transition-colors">
            Track another shipment →
          </a>
        </motion.div>
      </div>
    );
  }

  return (
    <div className="max-w-lg mx-auto px-4 py-12">
      {/* Header */}
      <motion.div initial={{ opacity: 0, y: -10 }} animate={{ opacity: 1, y: 0 }} className="text-center mb-10">
        <p className="font-mono text-xs text-white/30 tracking-widest uppercase mb-2">Delivery Feedback</p>
        <h1 className="text-2xl font-bold text-white font-space-grotesk">How was your delivery?</h1>
        <p className="text-white/40 text-sm mt-2">Tracking: <span className="font-mono text-[#00E5FF]">{AWB}</span></p>
      </motion.div>

      <AnimatePresence mode="wait">
        {step === "rate" && (
          <motion.div
            key="rate"
            initial={{ opacity: 0, x: 20 }}
            animate={{ opacity: 1, x: 0 }}
            exit={{ opacity: 0, x: -20 }}
            className="space-y-6"
          >
            {/* Overall rating */}
            <div
              className="p-6 rounded-2xl text-center"
              style={{ background: "rgba(255,255,255,0.04)", border: "1px solid rgba(255,255,255,0.08)" }}
            >
              <p className="text-white/60 text-sm mb-4">Overall experience</p>
              <div className="flex justify-center">
                <StarRating value={overallRating} onChange={setOverall} size={36} />
              </div>
              <p className="text-xs text-white/30 mt-3 font-mono">
                {overallRating === 0 ? "Tap to rate" :
                 overallRating === 1 ? "Very Poor" :
                 overallRating === 2 ? "Poor" :
                 overallRating === 3 ? "Average" :
                 overallRating === 4 ? "Good" : "Excellent!"}
              </p>
            </div>

            {/* Per-aspect ratings */}
            <div className="space-y-3">
              {ASPECT_LABELS.map(({ key, icon: Icon, label }) => (
                <div
                  key={key}
                  className="flex items-center justify-between p-4 rounded-xl"
                  style={{ background: "rgba(255,255,255,0.03)", border: "1px solid rgba(255,255,255,0.06)" }}
                >
                  <div className="flex items-center gap-3">
                    <Icon size={16} color="rgba(255,255,255,0.4)" />
                    <span className="text-sm text-white/70">{label}</span>
                  </div>
                  <StarRating
                    value={aspects[key as keyof typeof aspects]}
                    onChange={(n) => setAspects((p) => ({ ...p, [key]: n }))}
                    size={20}
                  />
                </div>
              ))}
            </div>

            <button
              onClick={() => setStep("issues")}
              disabled={overallRating === 0}
              className="w-full py-3.5 rounded-xl font-semibold text-sm text-[#050810] transition-all"
              style={{
                background: overallRating > 0 ? `linear-gradient(135deg, ${GREEN}, ${CYAN})` : "rgba(255,255,255,0.1)",
                color: overallRating > 0 ? "#050810" : "rgba(255,255,255,0.3)",
              }}
            >
              Next →
            </button>
          </motion.div>
        )}

        {step === "issues" && (
          <motion.div
            key="issues"
            initial={{ opacity: 0, x: 20 }}
            animate={{ opacity: 1, x: 0 }}
            exit={{ opacity: 0, x: -20 }}
            className="space-y-6"
          >
            {overallRating <= 3 && (
              <div>
                <div className="flex items-center gap-2 mb-4">
                  <AlertTriangle size={16} color={AMBER} />
                  <p className="text-sm text-white/70">What went wrong? (select all that apply)</p>
                </div>
                <div className="grid grid-cols-2 gap-2">
                  {ISSUE_OPTIONS.map((opt) => {
                    const selected = issues.includes(opt.id);
                    return (
                      <button
                        key={opt.id}
                        onClick={() => toggleIssue(opt.id)}
                        className="p-3 rounded-xl text-left text-xs transition-all"
                        style={{
                          background: selected ? `${RED}15` : "rgba(255,255,255,0.03)",
                          border: `1px solid ${selected ? `${RED}40` : "rgba(255,255,255,0.06)"}`,
                          color: selected ? RED : "rgba(255,255,255,0.6)",
                        }}
                      >
                        {opt.label}
                      </button>
                    );
                  })}
                </div>
              </div>
            )}

            <div>
              <p className="text-sm text-white/60 mb-2">{overallRating >= 4 ? "Tell us what you loved (optional)" : "Additional comments (optional)"}</p>
              <textarea
                value={comment}
                onChange={(e) => setComment(e.target.value)}
                placeholder={overallRating >= 4 ? "Your courier was great! The package arrived..." : "Please share more details..."}
                rows={4}
                className="w-full rounded-xl p-4 text-sm resize-none outline-none transition-all placeholder:text-white/20"
                style={{
                  background: "rgba(255,255,255,0.03)",
                  border: "1px solid rgba(255,255,255,0.08)",
                  color: "#fff",
                }}
                onFocus={(e) => (e.target.style.borderColor = "rgba(0,229,255,0.4)")}
                onBlur={(e) => (e.target.style.borderColor = "rgba(255,255,255,0.08)")}
              />
            </div>

            {error && (
              <div className="flex items-center gap-2 p-3 rounded-xl" style={{ background: "rgba(255,59,92,0.08)", border: "1px solid rgba(255,59,92,0.2)" }}>
                <XCircle size={14} color="#FF3B5C" className="shrink-0" />
                <p className="text-xs text-white/70">{error}</p>
              </div>
            )}

            <div className="flex gap-3">
              <button
                onClick={() => setStep("rate")}
                disabled={isPending}
                className="flex-[0.4] py-3.5 rounded-xl text-sm font-medium text-white/50 transition-all disabled:opacity-50"
                style={{ background: "rgba(255,255,255,0.05)", border: "1px solid rgba(255,255,255,0.08)" }}
              >
                ← Back
              </button>
              <button
                onClick={handleSubmit}
                disabled={isPending}
                className="flex-1 py-3.5 rounded-xl font-semibold text-sm text-[#050810] disabled:opacity-60"
                style={{ background: `linear-gradient(135deg, ${GREEN}, ${CYAN})` }}
              >
                {isPending ? "Submitting…" : "Submit Feedback"}
              </button>
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}

export default function FeedbackPage() {
  return (
    <Suspense>
      <FeedbackContent />
    </Suspense>
  );
}
