"use client";

import { useRef, useEffect, useState } from "react";

const metrics = [
  { value: 2400000, suffix: "+", label: "Shipments Processed", color: "text-cyan-neon", decimals: 0, divisor: 1000000, unit: "M" },
  { value: 98.7, suffix: "%", label: "On-Time Delivery Rate", color: "text-green-signal", decimals: 1, divisor: 1, unit: "" },
  { value: 340, suffix: "ms", label: "Avg AI Dispatch Time", color: "text-purple-plasma", decimals: 0, divisor: 1, unit: "" },
  { value: 60, suffix: "+", label: "Countries Active", color: "text-amber-signal", decimals: 0, divisor: 1, unit: "" },
  { value: 34, suffix: "%", label: "Revenue Uplift (Avg)", color: "text-cyan-neon", decimals: 0, divisor: 1, unit: "" },
  { value: 99.95, suffix: "%", label: "Platform Uptime SLA", color: "text-green-signal", decimals: 2, divisor: 1, unit: "" },
];

function useCountUp(target: number, duration = 2000, active: boolean) {
  const [count, setCount] = useState(0);
  useEffect(() => {
    if (!active) return;
    let start = 0;
    const step = target / (duration / 16);
    const timer = setInterval(() => {
      start += step;
      if (start >= target) { setCount(target); clearInterval(timer); }
      else setCount(start);
    }, 16);
    return () => clearInterval(timer);
  }, [target, duration, active]);
  return count;
}

function MetricCard({ value, suffix, label, color, decimals, divisor, unit }: typeof metrics[0]) {
  const ref = useRef<HTMLDivElement>(null);
  const [active, setActive] = useState(false);
  useEffect(() => {
    const obs = new IntersectionObserver(([e]) => { if (e.isIntersecting) setActive(true); }, { threshold: 0.3 });
    if (ref.current) obs.observe(ref.current);
    return () => obs.disconnect();
  }, []);
  const count = useCountUp(value, 1800, active);
  const display = divisor > 1
    ? (count / divisor).toFixed(decimals) + unit
    : count.toFixed(decimals) + unit;

  return (
    <div
      ref={ref}
      className="text-center glass-panel rounded-2xl border border-white/[0.06] p-6 hover:border-white/[0.12] transition-all duration-300"
    >
      <div
        className={`text-4xl lg:text-5xl font-bold ${color} mb-1`}
        style={{ fontFamily: "'Space Grotesk', sans-serif" }}
      >
        {display}{suffix}
      </div>
      <div className="text-xs text-slate-500 mt-1">{label}</div>
    </div>
  );
}

export default function Metrics() {
  return (
    <section className="py-20 lg:py-28 relative">
      <div className="absolute top-0 left-0 right-0 h-px bg-gradient-to-r from-transparent via-cyan-neon/20 to-transparent" />
      <div className="absolute bottom-0 left-0 right-0 h-px bg-gradient-to-r from-transparent via-purple-plasma/20 to-transparent" />

      <div className="max-w-6xl mx-auto px-6 lg:px-8">
        <div className="text-center mb-12">
          <h2
            className="text-3xl lg:text-4xl font-bold text-white"
            style={{ fontFamily: "'Space Grotesk', sans-serif" }}
          >
            Numbers That{" "}
            <span className="text-gradient-brand">Speak</span>
          </h2>
        </div>
        <div className="grid grid-cols-2 md:grid-cols-3 gap-4">
          {metrics.map((m) => (
            <MetricCard key={m.label} {...m} />
          ))}
        </div>
      </div>
    </section>
  );
}
