"use client";

import { Star } from "lucide-react";

const testimonials = [
  {
    quote: "We replaced 4 separate logistics tools with CargoMarket. Our dispatch time dropped from 8 minutes to under 30 seconds. The AI just works.",
    name: "Maria Santos",
    role: "COO, SwiftShip Philippines",
    segment: "Medium Business",
    color: "cyan",
    rating: 5,
  },
  {
    quote: "As a small courier, I couldn't afford enterprise software. CargoMarket's free tier gave me a professional tracking page and driver app on day one.",
    name: "Ahmed Al-Rashid",
    role: "Founder, QuickRun UAE",
    segment: "Small Business",
    color: "green",
    rating: 5,
  },
  {
    quote: "The white-label platform lets us offer CargoMarket under our own brand to our merchant clients. It's a genuine competitive advantage.",
    name: "Jennifer Lim",
    role: "VP Operations, LogiCorp Asia",
    segment: "Enterprise",
    color: "purple",
    rating: 5,
  },
  {
    quote: "The AI dispatch agent cut our fuel costs by 22% in the first month. I didn't expect ROI this fast.",
    name: "Carlos Mendez",
    role: "Fleet Manager, MexiLogistics",
    segment: "Medium Business",
    color: "amber",
    rating: 5,
  },
  {
    quote: "Customer complaints about 'where is my order' dropped by 80% after we enabled the WhatsApp notification flows. Magic.",
    name: "Priya Nair",
    role: "Customer Experience Lead, DeliverIN",
    segment: "Medium Business",
    color: "cyan",
    rating: 5,
  },
  {
    quote: "We process 40,000 shipments a day. CargoMarket handles it without breaking a sweat. P99 latency under 200ms, every time.",
    name: "Thomas Weber",
    role: "CTO, EuroFreight GmbH",
    segment: "Enterprise",
    color: "purple",
    rating: 5,
  },
];

const colorMap: Record<string, { border: string; badge: string; star: string }> = {
  cyan:   { border: "border-cyan-neon/15",    badge: "bg-cyan-neon/10 text-cyan-neon border-cyan-neon/20",       star: "text-cyan-neon" },
  green:  { border: "border-green-signal/15", badge: "bg-green-signal/10 text-green-signal border-green-signal/20", star: "text-green-signal" },
  purple: { border: "border-purple-plasma/15",badge: "bg-purple-plasma/10 text-purple-plasma border-purple-plasma/20", star: "text-purple-plasma" },
  amber:  { border: "border-amber-signal/15", badge: "bg-amber-signal/10 text-amber-signal border-amber-signal/20",  star: "text-amber-signal" },
};

export default function Testimonials() {
  return (
    <section className="py-24 lg:py-32 relative">
      <div className="max-w-7xl mx-auto px-6 lg:px-8">
        <div className="text-center mb-16">
          <h2
            className="text-4xl lg:text-5xl font-bold text-white mb-4"
            style={{ fontFamily: "'Space Grotesk', sans-serif" }}
          >
            Loved by{" "}
            <span className="text-gradient-brand">Logistics Teams</span>
          </h2>
          <p className="text-slate-400 text-lg">
            From solo couriers to enterprise fleets across 60 countries.
          </p>
        </div>

        <div className="grid md:grid-cols-2 lg:grid-cols-3 gap-4">
          {testimonials.map((t) => {
            const c = colorMap[t.color];
            return (
              <div
                key={t.name}
                className={`glass-panel rounded-2xl border ${c.border} p-6 hover:bg-white/[0.03] transition-all duration-300`}
              >
                {/* Stars */}
                <div className="flex gap-0.5 mb-4">
                  {Array.from({ length: t.rating }).map((_, i) => (
                    <Star key={i} className={`w-3.5 h-3.5 fill-current ${c.star}`} />
                  ))}
                </div>

                <blockquote className="text-sm text-slate-300 leading-relaxed mb-5">
                  &ldquo;{t.quote}&rdquo;
                </blockquote>

                <div className="flex items-center justify-between pt-4 border-t border-white/[0.06]">
                  <div>
                    <div className="text-sm font-semibold text-white">{t.name}</div>
                    <div className="text-xs text-slate-500 mt-0.5">{t.role}</div>
                  </div>
                  <span className={`px-2.5 py-1 rounded-full border text-xs font-medium ${c.badge}`}>
                    {t.segment}
                  </span>
                </div>
              </div>
            );
          })}
        </div>
      </div>
    </section>
  );
}
