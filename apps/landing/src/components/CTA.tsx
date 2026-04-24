"use client";

import { ArrowRight, Zap } from "lucide-react";

export default function CTA() {
  return (
    <section className="py-24 lg:py-32 relative overflow-hidden">
      {/* Glow background */}
      <div className="absolute inset-0 bg-grid-pattern bg-grid-md opacity-40 pointer-events-none" />
      <div className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 w-[700px] h-[300px] bg-cyan-neon/8 rounded-full blur-[100px] pointer-events-none" />
      <div className="absolute top-1/2 left-1/3 -translate-y-1/2 w-[400px] h-[250px] bg-purple-plasma/6 rounded-full blur-[80px] pointer-events-none" />

      <div className="max-w-4xl mx-auto px-6 lg:px-8 text-center relative z-10">
        {/* Badge */}
        <div className="inline-flex items-center gap-2 px-4 py-1.5 rounded-full glass-panel border border-cyan-neon/25 mb-8 text-xs font-medium text-cyan-neon">
          <span className="w-1.5 h-1.5 rounded-full bg-cyan-neon animate-pulse-neon" />
          No credit card required · Free forever plan
        </div>

        <h2
          className="text-5xl lg:text-6xl font-bold mb-6 leading-tight"
          style={{ fontFamily: "'Space Grotesk', sans-serif" }}
        >
          <span className="text-white">Ready to ship </span>
          <span className="text-shimmer">smarter?</span>
        </h2>

        <p className="text-xl text-slate-400 mb-10 max-w-2xl mx-auto leading-relaxed">
          Join thousands of businesses that have unified their logistics,
          reduced costs, and delighted customers with CargoMarket.
        </p>

        <div className="flex flex-col sm:flex-row items-center justify-center gap-4 mb-10">
          <a
            href="#pricing"
            className="group inline-flex items-center gap-2.5 px-8 py-4 rounded-xl text-base font-bold bg-gradient-to-r from-cyan-neon to-purple-plasma text-[#050810] hover:shadow-glow-cyan transition-all duration-300 hover:scale-[1.03]"
          >
            <Zap className="w-4 h-4" />
            Start For Free Today
            <ArrowRight className="w-4 h-4 group-hover:translate-x-1 transition-transform" />
          </a>
          <a
            href="mailto:support@cargomarket.net?subject=CargoMarket%20Demo%20Request"
            className="inline-flex items-center gap-2 px-8 py-4 rounded-xl text-base font-medium glass-panel border border-white/10 text-slate-300 hover:text-white hover:border-white/20 transition-all duration-300"
          >
            Schedule a Demo
          </a>
        </div>

        <div className="flex flex-wrap items-center justify-center gap-6 text-xs text-slate-600">
          {[
            "No credit card required",
            "Setup in 5 minutes",
            "Cancel anytime",
            "GDPR compliant",
            "99.9% uptime SLA",
          ].map((item) => (
            <span key={item} className="flex items-center gap-1.5">
              <span className="w-1 h-1 rounded-full bg-green-signal/60" />
              {item}
            </span>
          ))}
        </div>
      </div>
    </section>
  );
}
