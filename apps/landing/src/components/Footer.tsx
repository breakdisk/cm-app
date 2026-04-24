"use client";

import Link from "next/link";
import { Zap, Twitter, Linkedin, Github, Mail } from "lucide-react";

// Anchors on the landing page we already render. Unknown destinations route
// to `/` rather than `#` so clicks at least reset to the top; replace with
// real routes (/about, /careers, /blog, etc.) as those pages land.
const LANDING_HOME = "/";
const footerLinks: Record<string, Array<{ label: string; href: string }>> = {
  Platform: [
    { label: "Dispatch & Routing", href: `${LANDING_HOME}#features` },
    { label: "Driver App",         href: `${LANDING_HOME}#features` },
    { label: "Customer Portal",    href: `${LANDING_HOME}#features` },
    { label: "Analytics",          href: `${LANDING_HOME}#features` },
    { label: "AI Agents",          href: `${LANDING_HOME}#features` },
    { label: "Compliance",         href: `${LANDING_HOME}#features` },
  ],
  Company: [
    { label: "About Us",   href: LANDING_HOME },
    { label: "Careers",    href: LANDING_HOME },
    { label: "Blog",       href: LANDING_HOME },
    { label: "Press Kit",  href: LANDING_HOME },
    { label: "Partners",   href: LANDING_HOME },
    { label: "Contact",    href: "mailto:support@cargomarket.net" },
  ],
  Resources: [
    { label: "Documentation", href: LANDING_HOME },
    { label: "API Reference", href: LANDING_HOME },
    { label: "Changelog",     href: LANDING_HOME },
    { label: "Status Page",   href: LANDING_HOME },
    { label: "Community",     href: LANDING_HOME },
    { label: "Webinars",      href: LANDING_HOME },
  ],
  Legal: [
    { label: "Privacy Policy",    href: LANDING_HOME },
    { label: "Terms of Service",  href: LANDING_HOME },
    { label: "Cookie Policy",     href: LANDING_HOME },
    { label: "GDPR",              href: LANDING_HOME },
    { label: "Security",          href: LANDING_HOME },
    { label: "PCI-DSS",           href: LANDING_HOME },
  ],
};

const SOCIAL_LINKS: Array<{ Icon: typeof Twitter; href: string; label: string }> = [
  { Icon: Twitter,  href: "https://twitter.com",  label: "Twitter"  },
  { Icon: Linkedin, href: "https://linkedin.com", label: "LinkedIn" },
  { Icon: Github,   href: "https://github.com",   label: "GitHub"   },
  { Icon: Mail,     href: "mailto:support@cargomarket.net", label: "Email" },
];

export default function Footer() {
  return (
    <footer className="border-t border-white/[0.06] pt-16 pb-8 relative">
      <div className="absolute top-0 left-0 right-0 h-px bg-gradient-to-r from-transparent via-cyan-neon/15 to-transparent" />

      <div className="max-w-7xl mx-auto px-6 lg:px-8">
        {/* Top row */}
        <div className="grid lg:grid-cols-5 gap-10 mb-12">
          {/* Brand */}
          <div className="lg:col-span-1">
            <Link href="/" className="flex items-center gap-2.5 mb-4">
              <div className="w-8 h-8 rounded-lg bg-gradient-to-br from-cyan-neon/30 to-purple-plasma/30 flex items-center justify-center">
                <Zap className="w-4 h-4 text-cyan-neon" strokeWidth={2.5} />
              </div>
              <span
                className="text-lg font-bold"
                style={{ fontFamily: "'Space Grotesk', sans-serif" }}
              >
                <span className="text-gradient-brand">Cargo</span>
                <span className="text-white">Market</span>
              </span>
            </Link>
            <p className="text-xs text-slate-500 leading-relaxed mb-5">
              One AI logistics platform for small, medium & enterprise businesses.
              Ship smarter. Grow faster.
            </p>
            <div className="flex items-center gap-3">
              {SOCIAL_LINKS.map(({ Icon, href, label }) => (
                <a
                  key={label}
                  href={href}
                  target="_blank"
                  rel="noopener noreferrer"
                  aria-label={label}
                  className="w-8 h-8 rounded-lg glass-panel border border-white/[0.08] flex items-center justify-center text-slate-500 hover:text-cyan-neon hover:border-cyan-neon/30 transition-all duration-200"
                >
                  <Icon className="w-3.5 h-3.5" />
                </a>
              ))}
            </div>
          </div>

          {/* Links */}
          {Object.entries(footerLinks).map(([group, links]) => (
            <div key={group}>
              <h4
                className="text-xs font-semibold text-white uppercase tracking-wider mb-4"
                style={{ fontFamily: "'Space Grotesk', sans-serif" }}
              >
                {group}
              </h4>
              <ul className="space-y-2.5">
                {links.map(({ label, href }) => {
                  const isExternal = href.startsWith("mailto:") || href.startsWith("http");
                  const className = "text-xs text-slate-500 hover:text-slate-300 transition-colors duration-200";
                  return (
                    <li key={label}>
                      {isExternal ? (
                        <a href={href} className={className}>{label}</a>
                      ) : (
                        <Link href={href} className={className}>{label}</Link>
                      )}
                    </li>
                  );
                })}
              </ul>
            </div>
          ))}
        </div>

        {/* Bottom row */}
        <div className="pt-6 border-t border-white/[0.05] flex flex-col sm:flex-row items-center justify-between gap-4">
          <p className="text-xs text-slate-600">
            © {new Date().getFullYear()} CargoMarket. All rights reserved.
          </p>
          <div className="flex items-center gap-4">
            <span className="flex items-center gap-1.5 text-xs text-slate-600">
              <span className="w-1.5 h-1.5 rounded-full bg-green-signal animate-pulse-neon" />
              All systems operational
            </span>
            <span className="text-xs text-slate-700">|</span>
            <span
              className="text-xs text-slate-600"
              style={{ fontFamily: "'JetBrains Mono', monospace" }}
            >
              v2.4.1
            </span>
          </div>
        </div>
      </div>
    </footer>
  );
}
