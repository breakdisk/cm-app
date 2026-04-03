"use client";
/**
 * Customer Portal — Reschedule Delivery
 * Customer self-serve: pick a new delivery date and time window after a failed attempt.
 */
import { Suspense } from "react";
import { useState, useTransition } from "react";
import { useSearchParams } from "next/navigation";
import { motion, AnimatePresence } from "framer-motion";
import { Calendar, Clock, CheckCircle2, AlertCircle, XCircle } from "lucide-react";

const CYAN   = "#00E5FF";
const GREEN  = "#00FF88";
const PURPLE = "#A855F7";
const AMBER  = "#FFAB00";

const API_BASE = process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:8000";

// Generate next 7 business days from today (real date)
function getBusinessDays(count: number): Date[] {
  const days: Date[] = [];
  const d = new Date();
  while (days.length < count) {
    d.setDate(d.getDate() + 1);
    if (d.getDay() !== 0 && d.getDay() !== 6) days.push(new Date(d));
  }
  return days;
}

const BUSINESS_DAYS = getBusinessDays(7);

const TIME_WINDOWS = [
  { id: "morning",   label: "Morning",   sub: "8:00 AM – 12:00 PM" },
  { id: "afternoon", label: "Afternoon", sub: "12:00 PM – 5:00 PM" },
  { id: "anytime",   label: "Anytime",   sub: "Best available slot" },
];

const DAY_ABBR = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTH_NAMES = ["Jan","Feb","Mar","Apr","May","Jun","Jul","Aug","Sep","Oct","Nov","Dec"];

function RescheduleContent() {
  const searchParams = useSearchParams();
  const AWB = (searchParams.get("tn") ?? "").toUpperCase();

  const [step, setStep]           = useState<"pick" | "confirm" | "done">("pick");
  const [selectedDay, setDay]     = useState<Date | null>(null);
  const [timeWindow, setWindow]   = useState<string>("");
  const [error, setError]         = useState<string | null>(null);
  const [isPending, startTransition] = useTransition();

  const canProceed = selectedDay !== null && timeWindow !== "";

  function handleConfirm() {
    if (!selectedDay || !timeWindow || !AWB) return;
    setError(null);
    startTransition(async () => {
      try {
        const preferred_date = selectedDay.toISOString().split("T")[0];
        const slot = timeWindow === "anytime" ? undefined : timeWindow as "morning" | "afternoon";
        const res = await fetch(
          `${API_BASE}/v1/tracking/${encodeURIComponent(AWB)}/reschedule`,
          {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({ preferred_date, preferred_time_slot: slot }),
          }
        );
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        setStep("done");
      } catch {
        setError("Couldn't reschedule. Please try again or contact support.");
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
            style={{ background: `${CYAN}15`, border: `1px solid ${CYAN}40` }}
          >
            <CheckCircle2 size={36} color={CYAN} />
          </div>
          <h2 className="text-2xl font-bold text-white font-space-grotesk mb-3">Rescheduled!</h2>
          <p className="text-white/50 text-sm leading-relaxed mb-4">
            Your delivery <span className="font-mono text-[#00E5FF]">{AWB}</span> has been rescheduled.
          </p>
          <div
            className="p-4 rounded-xl text-sm mb-6"
            style={{ background: "rgba(255,255,255,0.04)", border: "1px solid rgba(255,255,255,0.08)" }}
          >
            <p className="text-white font-semibold">
              {selectedDay ? `${DAY_ABBR[selectedDay.getDay()]}, ${MONTH_NAMES[selectedDay.getMonth()]} ${selectedDay.getDate()}` : ""}
            </p>
            <p className="text-white/40 text-xs mt-1">
              {TIME_WINDOWS.find((t) => t.id === timeWindow)?.sub ?? ""}
            </p>
          </div>
          <p className="text-xs text-white/30">You'll receive a confirmation via SMS and email.</p>
          <a href="/" className="mt-6 inline-block text-sm text-[#00E5FF] hover:text-[#00E5FF]/70 transition-colors">
            Track your shipment →
          </a>
        </motion.div>
      </div>
    );
  }

  return (
    <div className="max-w-lg mx-auto px-4 py-12">
      {/* Header */}
      <motion.div initial={{ opacity: 0, y: -10 }} animate={{ opacity: 1, y: 0 }} className="mb-8">
        <p className="font-mono text-xs text-white/30 tracking-widest uppercase mb-2">Reschedule Delivery</p>
        <h1 className="text-2xl font-bold text-white font-space-grotesk">Pick a new date</h1>
        <p className="text-white/40 text-sm mt-2">
          Tracking: <span className="font-mono text-[#00E5FF]">{AWB}</span>
        </p>

        <div
          className="flex items-center gap-3 mt-4 p-3 rounded-xl"
          style={{ background: `${AMBER}10`, border: `1px solid ${AMBER}30` }}
        >
          <AlertCircle size={14} color={AMBER} className="shrink-0" />
          <p className="text-xs text-white/60">
            We attempted delivery on{" "}
            <span className="text-white">
              {new Date(Date.now() - 86400000).toLocaleDateString("en-PH", { month: "long", day: "numeric", year: "numeric" })}
            </span>{" "}
            but couldn't reach you.
          </p>
        </div>
      </motion.div>

      <AnimatePresence mode="wait">
        {step === "pick" && (
          <motion.div
            key="pick"
            initial={{ opacity: 0, x: 20 }}
            animate={{ opacity: 1, x: 0 }}
            exit={{ opacity: 0, x: -20 }}
            className="space-y-6"
          >
            {/* Date picker */}
            <div>
              <div className="flex items-center gap-2 mb-3">
                <Calendar size={14} color="rgba(255,255,255,0.4)" />
                <p className="text-sm text-white/60">Select a delivery date</p>
              </div>
              <div className="grid grid-cols-7 gap-1.5">
                {BUSINESS_DAYS.map((d) => {
                  const isSelected = selectedDay?.toDateString() === d.toDateString();
                  return (
                    <button
                      key={d.toISOString()}
                      onClick={() => setDay(d)}
                      className="flex flex-col items-center py-3 rounded-xl transition-all"
                      style={{
                        background: isSelected ? `${CYAN}15` : "rgba(255,255,255,0.03)",
                        border: `1px solid ${isSelected ? `${CYAN}40` : "rgba(255,255,255,0.06)"}`,
                      }}
                    >
                      <span className="text-[10px] text-white/30 font-mono">{DAY_ABBR[d.getDay()]}</span>
                      <span className={`text-sm font-bold mt-1 ${isSelected ? "text-[#00E5FF]" : "text-white"}`}>
                        {d.getDate()}
                      </span>
                      <span className="text-[10px] text-white/30">{MONTH_NAMES[d.getMonth()]}</span>
                    </button>
                  );
                })}
              </div>
            </div>

            {/* Time window */}
            <div>
              <div className="flex items-center gap-2 mb-3">
                <Clock size={14} color="rgba(255,255,255,0.4)" />
                <p className="text-sm text-white/60">Select a time window</p>
              </div>
              <div className="grid grid-cols-3 gap-2">
                {TIME_WINDOWS.map((tw) => {
                  const isSelected = timeWindow === tw.id;
                  return (
                    <button
                      key={tw.id}
                      onClick={() => setWindow(tw.id)}
                      className="p-4 rounded-xl text-left transition-all"
                      style={{
                        background: isSelected ? `${PURPLE}15` : "rgba(255,255,255,0.03)",
                        border: `1px solid ${isSelected ? `${PURPLE}40` : "rgba(255,255,255,0.06)"}`,
                      }}
                    >
                      <p className={`text-sm font-semibold ${isSelected ? "text-[#A855F7]" : "text-white"}`}>{tw.label}</p>
                      <p className="text-[11px] text-white/40 mt-0.5">{tw.sub}</p>
                    </button>
                  );
                })}
              </div>
            </div>

            <button
              onClick={() => setStep("confirm")}
              disabled={!canProceed}
              className="w-full py-3.5 rounded-xl font-semibold text-sm transition-all"
              style={{
                background: canProceed ? `linear-gradient(135deg, ${CYAN}, ${PURPLE})` : "rgba(255,255,255,0.08)",
                color: canProceed ? "#050810" : "rgba(255,255,255,0.3)",
              }}
            >
              Review →
            </button>
          </motion.div>
        )}

        {step === "confirm" && selectedDay && (
          <motion.div
            key="confirm"
            initial={{ opacity: 0, x: 20 }}
            animate={{ opacity: 1, x: 0 }}
            exit={{ opacity: 0, x: -20 }}
            className="space-y-6"
          >
            <div
              className="p-6 rounded-2xl"
              style={{ background: "rgba(255,255,255,0.04)", border: "1px solid rgba(255,255,255,0.08)" }}
            >
              <p className="text-xs text-white/30 font-mono uppercase tracking-widest mb-4">Confirm Reschedule</p>
              <div className="space-y-4">
                {[
                  { label: "Tracking Number", value: AWB,                                                                              mono: true  },
                  { label: "New Date",         value: `${DAY_ABBR[selectedDay.getDay()]}, ${MONTH_NAMES[selectedDay.getMonth()]} ${selectedDay.getDate()}, 2026`, mono: false },
                  { label: "Time Window",      value: TIME_WINDOWS.find((t) => t.id === timeWindow)?.sub ?? "",                       mono: false },
                ].map((row) => (
                  <div key={row.label} className="flex justify-between items-center py-3 border-b border-white/[0.06]">
                    <span className="text-xs text-white/40 uppercase tracking-wider font-mono">{row.label}</span>
                    <span className={`text-sm font-semibold ${row.mono ? "font-mono text-[#00E5FF]" : "text-white"}`}>{row.value}</span>
                  </div>
                ))}
              </div>

              <p className="text-xs text-white/30 mt-4">
                You can reschedule up to 2 more times for this shipment. After 3 failed attempts the package is returned to sender.
              </p>
            </div>

            {error && (
              <div className="flex items-center gap-2 p-3 rounded-xl" style={{ background: "rgba(255,59,92,0.08)", border: "1px solid rgba(255,59,92,0.2)" }}>
                <XCircle size={14} color="#FF3B5C" className="shrink-0" />
                <p className="text-xs text-white/70">{error}</p>
              </div>
            )}

            <div className="flex gap-3">
              <button
                onClick={() => setStep("pick")}
                disabled={isPending}
                className="flex-[0.4] py-3.5 rounded-xl text-sm font-medium text-white/50 transition-all disabled:opacity-50"
                style={{ background: "rgba(255,255,255,0.05)", border: "1px solid rgba(255,255,255,0.08)" }}
              >
                ← Edit
              </button>
              <button
                onClick={handleConfirm}
                disabled={isPending}
                className="flex-1 py-3.5 rounded-xl font-semibold text-sm text-[#050810] disabled:opacity-60"
                style={{ background: `linear-gradient(135deg, ${GREEN}, ${CYAN})` }}
              >
                {isPending ? "Saving…" : "Confirm Reschedule"}
              </button>
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}

export default function ReschedulePage() {
  return (
    <Suspense>
      <RescheduleContent />
    </Suspense>
  );
}
