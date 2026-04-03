"use client";

import { useState } from "react";
import { Check, Zap } from "lucide-react";

const plans = [
  {
    name: "Starter",
    desc: "For small businesses shipping up to 500 parcels/month.",
    monthly: 0,
    annual: 0,
    unit: "Free forever",
    color: "green",
    accent: "text-green-signal",
    border: "border-green-signal/20",
    badge: "bg-green-signal/10 text-green-signal border-green-signal/20",
    features: [
      "Up to 500 shipments/month",
      "1 branch / 5 drivers",
      "Basic tracking page",
      "WhatsApp + SMS notifications",
      "Mobile driver app",
      "Email support",
    ],
    cta: "Start Free",
    ctaStyle: "bg-green-signal/10 border border-green-signal/30 text-green-signal hover:bg-green-signal/20",
  },
  {
    name: "Growth",
    desc: "For growing businesses scaling their delivery operations.",
    monthly: 149,
    annual: 99,
    color: "cyan",
    accent: "text-cyan-neon",
    border: "border-cyan-neon/40",
    badge: "bg-cyan-neon/10 text-cyan-neon border-cyan-neon/20",
    featured: true,
    features: [
      "Up to 5,000 shipments/month",
      "3 branches / 50 drivers",
      "AI route optimization (VRP)",
      "Full CDP & analytics dashboard",
      "COD reconciliation & wallet",
      "Campaign automation",
      "Priority support + SLA",
    ],
    cta: "Start 14-Day Trial",
    ctaStyle: "bg-gradient-to-r from-cyan-neon to-purple-plasma text-[#050810] hover:scale-[1.02] font-bold",
  },
  {
    name: "Business",
    desc: "For established logistics operators with complex needs.",
    monthly: 499,
    annual: 349,
    color: "purple",
    accent: "text-purple-plasma",
    border: "border-purple-plasma/20",
    badge: "bg-purple-plasma/10 text-purple-plasma border-purple-plasma/20",
    features: [
      "Up to 50,000 shipments/month",
      "Unlimited branches & drivers",
      "Multi-carrier management",
      "AI agents (dispatch + support)",
      "Compliance & document module",
      "API access + webhooks",
      "Dedicated success manager",
    ],
    cta: "Start 14-Day Trial",
    ctaStyle: "bg-purple-plasma/10 border border-purple-plasma/30 text-purple-plasma hover:bg-purple-plasma/20",
  },
  {
    name: "Enterprise",
    desc: "Custom pricing for high-volume or multi-tenant deployments.",
    monthly: null,
    annual: null,
    color: "amber",
    accent: "text-amber-signal",
    border: "border-amber-signal/20",
    badge: "bg-amber-signal/10 text-amber-signal border-amber-signal/20",
    features: [
      "Unlimited shipments",
      "White-label platform",
      "Custom MCP agent integrations",
      "Dedicated infrastructure",
      "Custom AI model training",
      "99.99% uptime SLA",
      "24/7 dedicated support",
    ],
    cta: "Contact Sales",
    ctaStyle: "bg-amber-signal/10 border border-amber-signal/30 text-amber-signal hover:bg-amber-signal/20",
  },
];

export default function Pricing() {
  const [annual, setAnnual] = useState(true);

  return (
    <section id="pricing" className="py-24 lg:py-32 relative">
      <div className="absolute inset-0 bg-dot-pattern bg-dot-sm opacity-20 pointer-events-none" />

      <div className="max-w-7xl mx-auto px-6 lg:px-8 relative z-10">
        <div className="text-center mb-12">
          <div className="inline-flex items-center gap-2 px-4 py-1.5 rounded-full glass-panel border border-cyan-neon/20 mb-6 text-xs font-medium text-cyan-neon">
            <Zap className="w-3 h-3" />
            Transparent pricing. No hidden fees.
          </div>
          <h2
            className="text-4xl lg:text-5xl font-bold text-white mb-4"
            style={{ fontFamily: "'Space Grotesk', sans-serif" }}
          >
            Simple,{" "}
            <span className="text-gradient-brand">Scalable</span> Pricing
          </h2>
          <p className="text-slate-400 text-lg mb-8">
            Start free. Upgrade as you grow. Cancel anytime.
          </p>

          {/* Toggle */}
          <div className="inline-flex items-center gap-3 glass-panel rounded-full p-1 border border-white/[0.08]">
            <button
              onClick={() => setAnnual(false)}
              className={`px-4 py-1.5 rounded-full text-sm font-medium transition-all ${!annual ? "bg-white/[0.1] text-white" : "text-slate-500 hover:text-slate-300"}`}
            >
              Monthly
            </button>
            <button
              onClick={() => setAnnual(true)}
              className={`px-4 py-1.5 rounded-full text-sm font-medium transition-all flex items-center gap-2 ${annual ? "bg-white/[0.1] text-white" : "text-slate-500 hover:text-slate-300"}`}
            >
              Annual
              <span className="px-1.5 py-0.5 rounded text-xs bg-green-signal/20 text-green-signal font-semibold">
                Save 33%
              </span>
            </button>
          </div>
        </div>

        <div className="grid md:grid-cols-2 xl:grid-cols-4 gap-5">
          {plans.map((plan) => (
            <div
              key={plan.name}
              className={`relative glass-panel rounded-3xl border ${plan.border} p-6 flex flex-col transition-all duration-300 hover:scale-[1.01] ${
                plan.featured ? "shadow-glow-cyan xl:scale-[1.02] xl:hover:scale-[1.03]" : ""
              }`}
            >
              {plan.featured && (
                <div className="absolute -top-3.5 left-1/2 -translate-x-1/2 px-4 py-1 rounded-full bg-gradient-to-r from-cyan-neon to-purple-plasma text-[#050810] text-xs font-bold whitespace-nowrap">
                  Most Popular
                </div>
              )}

              <div className={`inline-flex items-center gap-1.5 px-3 py-1 rounded-full border text-xs font-medium ${plan.badge} mb-4 self-start`}>
                {plan.name}
              </div>

              {/* Price */}
              <div className="mb-4">
                {plan.monthly === null ? (
                  <div className={`text-3xl font-bold ${plan.accent}`} style={{ fontFamily: "'Space Grotesk', sans-serif" }}>
                    Custom
                  </div>
                ) : plan.monthly === 0 ? (
                  <div className={`text-3xl font-bold ${plan.accent}`} style={{ fontFamily: "'Space Grotesk', sans-serif" }}>
                    Free
                  </div>
                ) : (
                  <div className="flex items-end gap-1">
                    <span className={`text-4xl font-bold ${plan.accent}`} style={{ fontFamily: "'Space Grotesk', sans-serif" }}>
                      ${annual ? plan.annual : plan.monthly}
                    </span>
                    <span className="text-slate-500 text-sm mb-1">/mo</span>
                  </div>
                )}
                <p className="text-xs text-slate-500 mt-1 leading-relaxed">{plan.desc}</p>
              </div>

              <div className="h-px bg-white/[0.05] mb-5" />

              <ul className="space-y-2.5 mb-6 flex-1">
                {plan.features.map((f) => (
                  <li key={f} className="flex items-start gap-2 text-xs text-slate-400">
                    <Check className={`w-3.5 h-3.5 mt-0.5 flex-shrink-0 ${plan.accent}`} />
                    {f}
                  </li>
                ))}
              </ul>

              <a
                href="#"
                className={`block text-center py-3 rounded-xl text-sm transition-all duration-300 ${plan.ctaStyle}`}
              >
                {plan.cta}
              </a>
            </div>
          ))}
        </div>

        <p className="text-center text-xs text-slate-600 mt-8">
          All plans include SSL, 99.9% uptime, and free data migration assistance.
          No credit card required for free plan.
        </p>
      </div>
    </section>
  );
}
