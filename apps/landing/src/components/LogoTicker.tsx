"use client";

const brands = [
  "Shopify", "WooCommerce", "Lazada", "Shopee", "Stripe", "Twilio",
  "Mapbox", "AWS", "Google Maps", "PayMongo", "DHL", "FedEx",
  "Shopify", "WooCommerce", "Lazada", "Shopee", "Stripe", "Twilio",
  "Mapbox", "AWS", "Google Maps", "PayMongo", "DHL", "FedEx",
];

export default function LogoTicker() {
  return (
    <section className="py-12 border-y border-white/[0.05] overflow-hidden">
      <div className="max-w-7xl mx-auto px-6 mb-6 text-center">
        <p className="text-xs font-medium tracking-widest uppercase text-slate-600">
          Integrates with your entire stack
        </p>
      </div>
      <div className="relative flex">
        {/* Fade masks */}
        <div className="absolute left-0 top-0 bottom-0 w-32 z-10 bg-gradient-to-r from-[#050810] to-transparent pointer-events-none" />
        <div className="absolute right-0 top-0 bottom-0 w-32 z-10 bg-gradient-to-l from-[#050810] to-transparent pointer-events-none" />

        <div
          className="flex gap-8 whitespace-nowrap"
          style={{ animation: "ticker 28s linear infinite" }}
        >
          {brands.map((brand, i) => (
            <div
              key={`${brand}-${i}`}
              className="flex items-center gap-2 px-6 py-2.5 rounded-xl glass-panel border border-white/[0.06] text-sm font-medium text-slate-500 hover:text-slate-300 hover:border-white/[0.12] transition-all duration-200 cursor-default select-none"
            >
              <span className="w-1.5 h-1.5 rounded-full bg-cyan-neon/40" />
              {brand}
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}
