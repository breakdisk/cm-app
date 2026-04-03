"use client";

import { Store, Building2, Globe2, Check } from "lucide-react";

const tiers = [
  {
    icon: Store,
    segment: "Small Business",
    tagline: "Launch fast. Ship smarter.",
    color: "green",
    accent: "text-green-signal",
    border: "border-green-signal/20",
    glow: "hover:shadow-glow-green",
    bg: "bg-green-signal/5",
    badge: "bg-green-signal/10 text-green-signal border-green-signal/20",
    features: [
      "Self-service onboarding in 5 minutes",
      "Pay-per-shipment, no monthly minimums",
      "WhatsApp + SMS customer notifications",
      "Basic tracking page (white-labeled)",
      "Mobile driver app included",
    ],
    cta: "Start Free",
    ctaHref: "#pricing",
  },
  {
    icon: Building2,
    segment: "Medium Business",
    tagline: "Scale without complexity.",
    color: "cyan",
    accent: "text-cyan-neon",
    border: "border-cyan-neon/30",
    glow: "hover:shadow-glow-cyan",
    bg: "bg-cyan-neon/5",
    badge: "bg-cyan-neon/10 text-cyan-neon border-cyan-neon/20",
    featured: true,
    features: [
      "Multi-branch & multi-zone dispatch",
      "AI route optimization (VRP)",
      "Customer Data Platform (CDP)",
      "Campaign automation & re-engagement",
      "COD reconciliation & wallet payouts",
      "Analytics dashboard + BI exports",
    ],
    cta: "Most Popular",
    ctaHref: "#pricing",
  },
  {
    icon: Globe2,
    segment: "Enterprise",
    tagline: "Full platform. Your rules.",
    color: "purple",
    accent: "text-purple-plasma",
    border: "border-purple-plasma/20",
    glow: "hover:shadow-glow-purple",
    bg: "bg-purple-plasma/5",
    badge: "bg-purple-plasma/10 text-purple-plasma border-purple-plasma/20",
    features: [
      "Custom MCP agent integrations",
      "Multi-tenant white-label platform",
      "SLA enforcement & carrier management",
      "Dedicated compliance & audit module",
      "Dedicated infrastructure & SLAs",
      "Custom AI model training on your data",
    ],
    cta: "Contact Sales",
    ctaHref: "#contact",
  },
];

export default function PlatformTiers() {
  return (
    <section id="platform" className="py-24 lg:py-32 relative overflow-hidden">
      {/* Background */}
      <div className="absolute inset-0 bg-dot-pattern bg-dot-sm opacity-30 pointer-events-none" />

      <div className="max-w-7xl mx-auto px-6 lg:px-8 relative z-10">
        {/* Header */}
        <div className="text-center mb-16">
          <div className="inline-flex items-center gap-2 px-4 py-1.5 rounded-full glass-panel border border-purple-plasma/20 mb-6 text-xs font-medium text-purple-plasma">
            One platform, every scale
          </div>
          <h2
            className="text-4xl lg:text-5xl font-bold text-white mb-4 leading-tight"
            style={{ fontFamily: "'Space Grotesk', sans-serif" }}
          >
            Built for Every Business{" "}
            <span className="text-gradient-brand">Size</span>
          </h2>
          <p className="text-slate-400 text-lg max-w-2xl mx-auto">
            Whether you ship 10 parcels a day or 10 million, CargoMarket grows
            with you — no platform migration, no new vendor.
          </p>
        </div>

        {/* Tier cards */}
        <div className="grid lg:grid-cols-3 gap-6">
          {tiers.map((tier) => (
            <div
              key={tier.segment}
              className={`relative rounded-3xl glass-panel ${tier.border} border p-7 transition-all duration-300 ${tier.glow} ${
                tier.featured
                  ? "lg:scale-[1.03] border-cyan-neon/40 shadow-glow-cyan"
                  : ""
              }`}
            >
              {tier.featured && (
                <div className="absolute -top-3.5 left-1/2 -translate-x-1/2 px-4 py-1 rounded-full bg-gradient-to-r from-cyan-neon to-purple-plasma text-[#050810] text-xs font-bold whitespace-nowrap">
                  Most Popular
                </div>
              )}

              {/* Icon + segment */}
              <div className={`inline-flex items-center justify-center w-12 h-12 rounded-2xl ${tier.bg} border ${tier.border} mb-5`}>
                <tier.icon className={`w-6 h-6 ${tier.accent}`} />
              </div>

              <div className={`inline-flex items-center gap-1.5 px-3 py-1 rounded-full border text-xs font-medium ${tier.badge} mb-4`}>
                {tier.segment}
              </div>

              <h3
                className={`text-2xl font-bold ${tier.accent} mb-1`}
                style={{ fontFamily: "'Space Grotesk', sans-serif" }}
              >
                {tier.tagline}
              </h3>

              <div className="h-px bg-white/[0.06] my-5" />

              <ul className="space-y-3 mb-7">
                {tier.features.map((f) => (
                  <li key={f} className="flex items-start gap-2.5 text-sm text-slate-400">
                    <Check className={`w-4 h-4 mt-0.5 flex-shrink-0 ${tier.accent}`} />
                    {f}
                  </li>
                ))}
              </ul>

              <a
                href={tier.ctaHref}
                className={`block w-full text-center py-3 rounded-xl text-sm font-semibold transition-all duration-300 ${
                  tier.featured
                    ? "bg-gradient-to-r from-cyan-neon to-purple-plasma text-[#050810] hover:scale-[1.02]"
                    : `${tier.bg} border ${tier.border} ${tier.accent} hover:bg-opacity-80`
                }`}
              >
                {tier.cta}
              </a>
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}
