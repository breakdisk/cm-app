"use client";

import {
  Route, Bell, BarChart3, Shield, Smartphone, Repeat,
  MessageSquare, Wallet, PackageSearch, Users, Zap, Map,
} from "lucide-react";

const features = [
  {
    icon: Route,
    title: "AI Route Optimization",
    desc: "Vehicle Routing Problem (VRP) solved in <500ms. Reduce fuel costs by up to 28%.",
    color: "cyan",
  },
  {
    icon: Bell,
    title: "Automated Notifications",
    desc: "WhatsApp, SMS, Email & Push — triggered at every shipment milestone, automatically.",
    color: "green",
  },
  {
    icon: BarChart3,
    title: "Analytics & BI",
    desc: "Real-time delivery KPIs, zone performance, driver stats, and custom BI exports.",
    color: "purple",
  },
  {
    icon: Shield,
    title: "Proof of Delivery",
    desc: "Photo + signature + GPS + OTP capture. Dispute-proof, tamper-evident POD.",
    color: "amber",
  },
  {
    icon: Smartphone,
    title: "Driver Super App",
    desc: "Offline-first mobile app with barcode scanner, turn-by-turn navigation, and task queue.",
    color: "cyan",
  },
  {
    icon: Repeat,
    title: "Multi-Carrier Management",
    desc: "Auto-allocate shipments across your carrier network. AI picks the best carrier per route.",
    color: "green",
  },
  {
    icon: MessageSquare,
    title: "CDP & Customer Profiles",
    desc: "Unified customer identity across all touchpoints. Churn prediction, CLV modeling.",
    color: "purple",
  },
  {
    icon: Wallet,
    title: "COD & Payments",
    desc: "Cash on delivery collection, wallet payouts to drivers, instant merchant invoicing.",
    color: "amber",
  },
  {
    icon: PackageSearch,
    title: "Live Tracking",
    desc: "Real-time package location with branded customer tracking page. Sub-2s updates.",
    color: "cyan",
  },
  {
    icon: Users,
    title: "Multi-Tenant Architecture",
    desc: "Full data isolation per tenant via PostgreSQL Row-Level Security. GDPR compliant.",
    color: "green",
  },
  {
    icon: Zap,
    title: "Business Rules Engine",
    desc: "No-code automation rules: if order > 5kg → assign truck; if late → escalate + notify.",
    color: "purple",
  },
  {
    icon: Map,
    title: "Hub & Warehouse Ops",
    desc: "Dock scheduling, cross-docking management, inbound/outbound scan workflows.",
    color: "amber",
  },
];

const colorMap: Record<string, { icon: string; bg: string; border: string }> = {
  cyan:   { icon: "text-cyan-neon",    bg: "bg-cyan-neon/5",    border: "border-cyan-neon/15" },
  green:  { icon: "text-green-signal", bg: "bg-green-signal/5", border: "border-green-signal/15" },
  purple: { icon: "text-purple-plasma",bg: "bg-purple-plasma/5",border: "border-purple-plasma/15" },
  amber:  { icon: "text-amber-signal", bg: "bg-amber-signal/5", border: "border-amber-signal/15" },
};

export default function Features() {
  return (
    <section id="features" className="py-24 lg:py-32 relative">
      {/* Ambient */}
      <div className="absolute top-0 left-1/2 -translate-x-1/2 w-[600px] h-px bg-gradient-to-r from-transparent via-cyan-neon/20 to-transparent" />

      <div className="max-w-7xl mx-auto px-6 lg:px-8">
        <div className="text-center mb-16">
          <div className="inline-flex items-center gap-2 px-4 py-1.5 rounded-full glass-panel border border-green-signal/20 mb-6 text-xs font-medium text-green-signal">
            Everything you need
          </div>
          <h2
            className="text-4xl lg:text-5xl font-bold text-white mb-4"
            style={{ fontFamily: "'Space Grotesk', sans-serif" }}
          >
            The Complete{" "}
            <span className="text-gradient-cyan-purple">Logistics Stack</span>
          </h2>
          <p className="text-slate-400 text-lg max-w-2xl mx-auto">
            Replace 7 separate tools with one integrated platform. No more data
            silos, no more context-switching.
          </p>
        </div>

        <div className="grid sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4">
          {features.map((f, i) => {
            const c = colorMap[f.color];
            return (
              <div
                key={f.title}
                className={`group glass-panel rounded-2xl border ${c.border} p-5 hover:${c.bg} transition-all duration-300 cursor-default`}
                style={{ animationDelay: `${i * 0.05}s` }}
              >
                <div className={`inline-flex items-center justify-center w-10 h-10 rounded-xl ${c.bg} border ${c.border} mb-4 group-hover:scale-110 transition-transform duration-300`}>
                  <f.icon className={`w-5 h-5 ${c.icon}`} />
                </div>
                <h3 className="text-sm font-semibold text-white mb-1.5">{f.title}</h3>
                <p className="text-xs text-slate-500 leading-relaxed">{f.desc}</p>
              </div>
            );
          })}
        </div>
      </div>
    </section>
  );
}
