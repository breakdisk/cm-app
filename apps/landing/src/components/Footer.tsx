"use client";

import { Zap, Twitter, Linkedin, Github, Mail } from "lucide-react";

const footerLinks = {
  Platform: ["Dispatch & Routing", "Driver App", "Customer Portal", "Analytics", "AI Agents", "Compliance"],
  Company: ["About Us", "Careers", "Blog", "Press Kit", "Partners", "Contact"],
  Resources: ["Documentation", "API Reference", "Changelog", "Status Page", "Community", "Webinars"],
  Legal: ["Privacy Policy", "Terms of Service", "Cookie Policy", "GDPR", "Security", "PCI-DSS"],
};

export default function Footer() {
  return (
    <footer className="border-t border-white/[0.06] pt-16 pb-8 relative">
      <div className="absolute top-0 left-0 right-0 h-px bg-gradient-to-r from-transparent via-cyan-neon/15 to-transparent" />

      <div className="max-w-7xl mx-auto px-6 lg:px-8">
        {/* Top row */}
        <div className="grid lg:grid-cols-5 gap-10 mb-12">
          {/* Brand */}
          <div className="lg:col-span-1">
            <a href="#" className="flex items-center gap-2.5 mb-4">
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
            </a>
            <p className="text-xs text-slate-500 leading-relaxed mb-5">
              One AI logistics platform for small, medium & enterprise businesses.
              Ship smarter. Grow faster.
            </p>
            <div className="flex items-center gap-3">
              {[Twitter, Linkedin, Github, Mail].map((Icon, i) => (
                <a
                  key={i}
                  href="#"
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
                {links.map((link) => (
                  <li key={link}>
                    <a
                      href="#"
                      className="text-xs text-slate-500 hover:text-slate-300 transition-colors duration-200"
                    >
                      {link}
                    </a>
                  </li>
                ))}
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
