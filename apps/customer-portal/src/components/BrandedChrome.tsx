"use client";

import { useBranding } from "@/lib/branding";

/** Brand-aware top nav for the customer tracking portal. */
export function BrandedNav() {
  const { branding } = useBranding();
  return (
    <nav className="relative z-10 flex items-center justify-between px-6 py-4 border-b border-white/5">
      <div className="flex items-center gap-2">
        {branding.logo_url ? (
          // eslint-disable-next-line @next/next/no-img-element
          <img
            src={branding.logo_url}
            alt={branding.display_name}
            className="h-7 w-7 rounded-lg object-contain"
          />
        ) : (
          <div
            className="h-7 w-7 rounded-lg flex items-center justify-center"
            style={{ background: "linear-gradient(135deg, var(--brand-primary), var(--brand-secondary))" }}
          >
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="#050810" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
              <path d="M5 18H3a2 2 0 01-2-2V8a2 2 0 012-2h3.19M15 6h2a2 2 0 012 2v8a2 2 0 01-2 2h-3.19" />
              <line x1="23" y1="13" x2="23" y2="11" />
              <polyline points="11 6 7 12 13 12 9 18" />
            </svg>
          </div>
        )}
        <span className="font-heading text-sm font-bold text-white">{branding.display_name}</span>
      </div>
      <span className="text-2xs font-mono text-white/30">Track · Reschedule · Feedback</span>
    </nav>
  );
}

/** Brand-aware footer. */
export function BrandedFooter() {
  const { branding } = useBranding();
  return (
    <footer className="relative z-10 border-t border-white/5 py-8 text-center">
      <p className="text-2xs font-mono text-white/20">
        Powered by {branding.display_name} · {new Date().getFullYear()}
      </p>
    </footer>
  );
}
