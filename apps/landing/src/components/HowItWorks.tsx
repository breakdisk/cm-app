"use client";

import { ShoppingBag, Cpu, Truck, CheckCircle2, MessageCircle, TrendingUp } from "lucide-react";

const steps = [
  {
    step: "01",
    icon: ShoppingBag,
    title: "Order Received",
    desc: "Order arrives via API, Shopify webhook, or merchant portal. AI validates, geocodes, and normalizes the address in milliseconds.",
    color: "cyan",
  },
  {
    step: "02",
    icon: Cpu,
    title: "AI Dispatches",
    desc: "The dispatch engine solves VRP across your driver fleet, factoring traffic, load capacity, and SLA windows — in under 500ms.",
    color: "purple",
  },
  {
    step: "03",
    icon: Truck,
    title: "Driver Picks Up",
    desc: "Driver gets optimized route on the super app. Real-time GPS feeds the tracking system. Customer gets WhatsApp ETA update.",
    color: "amber",
  },
  {
    step: "04",
    icon: CheckCircle2,
    title: "POD Captured",
    desc: "Photo + GPS + signature (or OTP) captured at doorstep. Dispute-proof proof of delivery stored immutably.",
    color: "green",
  },
  {
    step: "05",
    icon: MessageCircle,
    title: "Customer Engaged",
    desc: "Delivery confirmation sent. AI triggers next-shipment campaign based on CLV score and delivery behavior.",
    color: "cyan",
  },
  {
    step: "06",
    icon: TrendingUp,
    title: "Analytics Updated",
    desc: "Every event feeds your BI dashboard in real time. Zone demand forecasts update. Models retrain on new data.",
    color: "purple",
  },
];

const colorMap: Record<string, { text: string; border: string; bg: string; connector: string }> = {
  cyan:   { text: "text-cyan-neon",    border: "border-cyan-neon/30",    bg: "bg-cyan-neon/8",    connector: "bg-cyan-neon/20" },
  purple: { text: "text-purple-plasma",border: "border-purple-plasma/30",bg: "bg-purple-plasma/8",connector: "bg-purple-plasma/20" },
  amber:  { text: "text-amber-signal", border: "border-amber-signal/30", bg: "bg-amber-signal/8", connector: "bg-amber-signal/20" },
  green:  { text: "text-green-signal", border: "border-green-signal/30", bg: "bg-green-signal/8", connector: "bg-green-signal/20" },
};

export default function HowItWorks() {
  return (
    <section id="how-it-works" className="py-24 lg:py-32 relative">
      <div className="absolute inset-0 bg-gradient-to-b from-transparent via-[#080d1a]/60 to-transparent pointer-events-none" />

      <div className="max-w-6xl mx-auto px-6 lg:px-8 relative z-10">
        <div className="text-center mb-16">
          <div className="inline-flex items-center gap-2 px-4 py-1.5 rounded-full glass-panel border border-amber-signal/20 mb-6 text-xs font-medium text-amber-signal">
            End-to-end automation
          </div>
          <h2
            className="text-4xl lg:text-5xl font-bold text-white mb-4"
            style={{ fontFamily: "'Space Grotesk', sans-serif" }}
          >
            From Order to{" "}
            <span className="text-gradient-brand">Delivered</span>
          </h2>
          <p className="text-slate-400 text-lg max-w-2xl mx-auto">
            The entire delivery lifecycle — automated, AI-optimized, and
            customer-engaged. No manual steps.
          </p>
        </div>

        {/* Steps grid */}
        <div className="grid md:grid-cols-2 lg:grid-cols-3 gap-5">
          {steps.map((s, i) => {
            const c = colorMap[s.color];
            return (
              <div
                key={s.step}
                className={`relative glass-panel rounded-2xl border ${c.border} p-6 hover:${c.bg} transition-all duration-300 group`}
              >
                {/* Step number */}
                <div className="flex items-center justify-between mb-5">
                  <span
                    className={`text-4xl font-bold opacity-20 ${c.text}`}
                    style={{ fontFamily: "'JetBrains Mono', monospace" }}
                  >
                    {s.step}
                  </span>
                  <div className={`w-10 h-10 rounded-xl ${c.bg} border ${c.border} flex items-center justify-center group-hover:scale-110 transition-transform`}>
                    <s.icon className={`w-5 h-5 ${c.text}`} />
                  </div>
                </div>

                <h3
                  className={`text-base font-semibold ${c.text} mb-2`}
                  style={{ fontFamily: "'Space Grotesk', sans-serif" }}
                >
                  {s.title}
                </h3>
                <p className="text-sm text-slate-500 leading-relaxed">{s.desc}</p>

                {/* Connector dot */}
                {i < steps.length - 1 && (
                  <div className={`absolute -right-2.5 top-1/2 w-5 h-5 rounded-full ${c.connector} border ${c.border} hidden lg:flex items-center justify-center z-10`}>
                    <div className={`w-1.5 h-1.5 rounded-full ${c.text.replace("text-", "bg-")}`} />
                  </div>
                )}
              </div>
            );
          })}
        </div>
      </div>
    </section>
  );
}
