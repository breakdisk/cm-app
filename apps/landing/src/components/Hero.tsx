"use client";

import { useEffect, useRef } from "react";
import { ArrowRight, Play, TrendingUp, Truck, Brain, Globe } from "lucide-react";

const stats = [
  { value: "2.4M+", label: "Shipments Delivered", color: "text-cyan-neon" },
  { value: "98.7%", label: "On-time Rate", color: "text-green-signal" },
  { value: "340ms", label: "Avg Dispatch Time", color: "text-purple-plasma" },
  { value: "60+", label: "Countries", color: "text-amber-signal" },
];

const floatingCards = [
  {
    icon: Brain,
    title: "AI Dispatch",
    value: "12ms",
    sub: "Route optimization",
    color: "cyan",
    glow: "shadow-glow-cyan",
    position: "top-16 -left-4 lg:top-24 lg:-left-12",
  },
  {
    icon: TrendingUp,
    title: "Revenue +34%",
    value: "↑ 34%",
    sub: "Last 30 days",
    color: "green",
    glow: "shadow-glow-green",
    position: "top-8 -right-4 lg:top-16 lg:-right-12",
  },
  {
    icon: Truck,
    title: "Live Fleet",
    value: "1,204",
    sub: "Drivers active now",
    color: "purple",
    glow: "shadow-glow-purple",
    position: "bottom-16 -left-4 lg:bottom-24 lg:-left-16",
  },
  {
    icon: Globe,
    title: "Global Reach",
    value: "63",
    sub: "Active markets",
    color: "amber",
    glow: "shadow-glow-amber",
    position: "bottom-8 -right-4 lg:bottom-20 lg:-right-16",
  },
];

export default function Hero() {
  const gridRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (!gridRef.current) return;
      const { clientX: x, clientY: y } = e;
      const rect = gridRef.current.getBoundingClientRect();
      const px = ((x - rect.left) / rect.width) * 100;
      const py = ((y - rect.top) / rect.height) * 100;
      gridRef.current.style.setProperty("--mx", `${px}%`);
      gridRef.current.style.setProperty("--my", `${py}%`);
    };
    window.addEventListener("mousemove", handler);
    return () => window.removeEventListener("mousemove", handler);
  }, []);

  return (
    <section className="relative min-h-screen flex flex-col items-center justify-center overflow-hidden pt-16">
      {/* Animated grid background */}
      <div
        ref={gridRef}
        className="absolute inset-0 bg-grid-pattern bg-grid-md opacity-100"
        style={{
          maskImage:
            "radial-gradient(ellipse 80% 60% at var(--mx, 50%) var(--my, 50%), black 20%, transparent 70%)",
          WebkitMaskImage:
            "radial-gradient(ellipse 80% 60% at var(--mx, 50%) var(--my, 50%), black 20%, transparent 70%)",
        }}
      />

      {/* Ambient orbs */}
      <div className="absolute top-1/4 left-1/4 w-96 h-96 bg-cyan-neon/8 rounded-full blur-[120px] pointer-events-none" />
      <div className="absolute top-1/3 right-1/4 w-80 h-80 bg-purple-plasma/8 rounded-full blur-[100px] pointer-events-none" />
      <div className="absolute bottom-1/4 left-1/2 w-72 h-72 bg-green-signal/6 rounded-full blur-[100px] pointer-events-none" />

      <div className="relative z-10 max-w-7xl mx-auto px-6 lg:px-8 text-center">
        {/* Badge */}
        <div className="inline-flex items-center gap-2 px-4 py-1.5 rounded-full glass-panel border border-cyan-neon/20 mb-8 text-xs font-medium text-cyan-neon">
          <span className="w-1.5 h-1.5 rounded-full bg-cyan-neon animate-pulse-neon" />
          Now powering logistics in 60+ countries
          <ArrowRight className="w-3 h-3" />
        </div>

        {/* Headline */}
        <h1
          className="text-5xl sm:text-6xl lg:text-7xl xl:text-8xl font-bold tracking-tight leading-[0.95] mb-6"
          style={{ fontFamily: "'Space Grotesk', sans-serif" }}
        >
          <span className="block text-white">One Platform.</span>
          <span className="block text-shimmer mt-1">All Business Sizes.</span>
          <span className="block text-white mt-1">AI Logistics.</span>
        </h1>

        {/* Sub */}
        <p className="max-w-2xl mx-auto text-lg lg:text-xl text-slate-400 leading-relaxed mb-10">
          CargoMarket integrates{" "}
          <span className="text-white font-medium">small, medium & enterprise</span>{" "}
          businesses through a single AI-powered logistics platform —
          from first dispatch to last-mile delivery.
        </p>

        {/* CTA row */}
        <div className="flex flex-col sm:flex-row items-center justify-center gap-4 mb-16">
          <a
            href="#pricing"
            className="group inline-flex items-center gap-2 px-7 py-3.5 rounded-xl text-base font-semibold bg-gradient-to-r from-cyan-neon to-purple-plasma text-[#050810] hover:shadow-glow-cyan transition-all duration-300 hover:scale-[1.03]"
          >
            Start For Free
            <ArrowRight className="w-4 h-4 group-hover:translate-x-1 transition-transform" />
          </a>
          <a
            href="#how-it-works"
            className="inline-flex items-center gap-2.5 px-7 py-3.5 rounded-xl text-base font-medium glass-panel border border-white/10 text-slate-300 hover:text-white hover:border-cyan-neon/30 transition-all duration-300"
          >
            <div className="w-7 h-7 rounded-full bg-white/10 flex items-center justify-center">
              <Play className="w-3 h-3 text-white fill-white ml-0.5" />
            </div>
            Watch Demo
          </a>
        </div>

        {/* Stats row */}
        <div className="grid grid-cols-2 lg:grid-cols-4 gap-4 max-w-3xl mx-auto mb-20">
          {stats.map((stat) => (
            <div
              key={stat.label}
              className="glass-panel rounded-2xl p-4 text-center hover:bg-white/[0.06] transition-colors duration-300"
            >
              <div
                className={`text-2xl lg:text-3xl font-bold ${stat.color}`}
                style={{ fontFamily: "'Space Grotesk', sans-serif" }}
              >
                {stat.value}
              </div>
              <div className="text-xs text-slate-500 mt-1">{stat.label}</div>
            </div>
          ))}
        </div>

        {/* Dashboard mockup */}
        <div className="relative max-w-5xl mx-auto">
          {/* Floating cards */}
          {floatingCards.map((card) => (
            <FloatingCard key={card.title} {...card} />
          ))}

          {/* Main dashboard frame */}
          <div className="relative glass-panel rounded-2xl border border-white/10 overflow-hidden shadow-[0_0_80px_rgba(0,229,255,0.08),0_40px_120px_rgba(0,0,0,0.8)]">
            {/* Browser chrome */}
            <div className="flex items-center gap-2 px-4 py-3 border-b border-white/[0.06] bg-white/[0.02]">
              <div className="flex gap-1.5">
                <div className="w-3 h-3 rounded-full bg-red-signal/60" />
                <div className="w-3 h-3 rounded-full bg-amber-signal/60" />
                <div className="w-3 h-3 rounded-full bg-green-signal/60" />
              </div>
              <div className="flex-1 mx-4 px-3 py-1 rounded-md bg-white/[0.04] text-xs text-slate-500 text-left">
                app.cargomarket.io/dispatch
              </div>
            </div>

            {/* Dashboard content */}
            <div className="p-4 lg:p-6 bg-[#080d1a]">
              {/* Top bar */}
              <div className="flex items-center justify-between mb-5">
                <div>
                  <div className="text-white font-semibold text-sm">Dispatch Console</div>
                  <div className="text-xs text-slate-500 mt-0.5">Live · 1,204 drivers active</div>
                </div>
                <div className="flex items-center gap-2">
                  <span className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-green-signal/10 border border-green-signal/20 text-green-signal text-xs font-medium">
                    <span className="w-1.5 h-1.5 rounded-full bg-green-signal animate-pulse-neon" />
                    All Systems Operational
                  </span>
                </div>
              </div>

              {/* KPI row */}
              <div className="grid grid-cols-4 gap-3 mb-5">
                {[
                  { label: "Orders Today", val: "8,421", color: "text-cyan-neon", bar: "bg-cyan-neon" },
                  { label: "Delivered", val: "7,934", color: "text-green-signal", bar: "bg-green-signal" },
                  { label: "In Transit", val: "342", color: "text-purple-plasma", bar: "bg-purple-plasma" },
                  { label: "Exceptions", val: "145", color: "text-amber-signal", bar: "bg-amber-signal" },
                ].map((kpi) => (
                  <div key={kpi.label} className="rounded-xl bg-white/[0.03] border border-white/[0.06] p-3">
                    <div className="text-xs text-slate-500 mb-1.5">{kpi.label}</div>
                    <div className={`text-xl font-bold ${kpi.color}`} style={{ fontFamily: "'Space Grotesk', sans-serif" }}>{kpi.val}</div>
                    <div className="mt-2 h-1 rounded-full bg-white/[0.06]">
                      <div className={`h-full rounded-full ${kpi.bar} opacity-70`} style={{ width: "72%" }} />
                    </div>
                  </div>
                ))}
              </div>

              {/* Map placeholder + order list */}
              <div className="grid lg:grid-cols-3 gap-3">
                {/* Map */}
                <div className="lg:col-span-2 rounded-xl bg-[#0a0f1e] border border-white/[0.06] overflow-hidden relative" style={{ height: "200px" }}>
                  <div className="absolute inset-0 bg-dot-pattern bg-dot-sm opacity-50" />
                  {/* Simulated route lines */}
                  <svg className="absolute inset-0 w-full h-full" viewBox="0 0 400 200" preserveAspectRatio="none">
                    <path d="M 40 120 Q 120 40 200 90 Q 280 140 360 60" stroke="#00E5FF" strokeWidth="1.5" fill="none" strokeDasharray="4 3" opacity="0.5" />
                    <path d="M 80 160 Q 160 80 240 110 Q 320 140 380 90" stroke="#A855F7" strokeWidth="1.5" fill="none" strokeDasharray="4 3" opacity="0.4" />
                    <path d="M 20 80 Q 100 140 180 70 Q 260 20 340 100" stroke="#00FF88" strokeWidth="1" fill="none" strokeDasharray="3 4" opacity="0.3" />
                    {[
                      { cx: 200, cy: 90, color: "#00E5FF" },
                      { cx: 120, cy: 60, color: "#A855F7" },
                      { cx: 300, cy: 130, color: "#00FF88" },
                      { cx: 80,  cy: 140, color: "#FFAB00" },
                      { cx: 350, cy: 70,  color: "#00E5FF" },
                    ].map((dot, i) => (
                      <g key={i}>
                        <circle cx={dot.cx} cy={dot.cy} r="6" fill={dot.color} opacity="0.15" />
                        <circle cx={dot.cx} cy={dot.cy} r="3" fill={dot.color} opacity="0.9" />
                      </g>
                    ))}
                  </svg>
                  <div className="absolute bottom-2 left-2 text-xs text-slate-600">Live Driver Map</div>
                </div>

                {/* Order feed */}
                <div className="rounded-xl bg-white/[0.02] border border-white/[0.06] p-3 overflow-hidden">
                  <div className="text-xs font-medium text-slate-400 mb-3">Recent Orders</div>
                  <div className="flex flex-col gap-2">
                    {[
                      { id: "#AWB-8821", status: "Delivered", color: "text-green-signal bg-green-signal/10" },
                      { id: "#AWB-8820", status: "In Transit", color: "text-cyan-neon bg-cyan-neon/10" },
                      { id: "#AWB-8819", status: "Dispatched", color: "text-purple-plasma bg-purple-plasma/10" },
                      { id: "#AWB-8818", status: "Delivered", color: "text-green-signal bg-green-signal/10" },
                      { id: "#AWB-8817", status: "Exception", color: "text-amber-signal bg-amber-signal/10" },
                    ].map((order) => (
                      <div key={order.id} className="flex items-center justify-between py-1.5 border-b border-white/[0.04] last:border-0">
                        <span className="text-xs font-mono text-slate-400">{order.id}</span>
                        <span className={`text-xs px-2 py-0.5 rounded-full font-medium ${order.color}`}>{order.status}</span>
                      </div>
                    ))}
                  </div>
                </div>
              </div>
            </div>
          </div>

          {/* Bottom glow */}
          <div className="absolute -bottom-10 left-1/2 -translate-x-1/2 w-3/4 h-20 bg-cyan-neon/10 blur-3xl rounded-full pointer-events-none" />
        </div>
      </div>
    </section>
  );
}

function FloatingCard({
  icon: Icon, title, value, sub, color, glow, position,
}: {
  icon: React.ElementType; title: string; value: string; sub: string;
  color: string; glow: string; position: string;
}) {
  const colorMap: Record<string, string> = {
    cyan: "text-cyan-neon border-cyan-neon/20 bg-cyan-neon/5",
    green: "text-green-signal border-green-signal/20 bg-green-signal/5",
    purple: "text-purple-plasma border-purple-plasma/20 bg-purple-plasma/5",
    amber: "text-amber-signal border-amber-signal/20 bg-amber-signal/5",
  };
  return (
    <div
      className={`absolute hidden lg:flex z-20 items-center gap-3 px-4 py-3 rounded-2xl glass-panel-bright border ${colorMap[color]} ${glow} ${position} animate-float`}
      style={{ animationDelay: `${Math.random() * 2}s` }}
    >
      <Icon className={`w-5 h-5 ${colorMap[color].split(" ")[0]}`} />
      <div>
        <div className="text-xs text-slate-500">{title}</div>
        <div className={`text-base font-bold ${colorMap[color].split(" ")[0]}`} style={{ fontFamily: "'Space Grotesk', sans-serif" }}>{value}</div>
        <div className="text-xs text-slate-600">{sub}</div>
      </div>
    </div>
  );
}
