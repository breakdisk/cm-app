/**
 * Customer Portal — Root Layout
 * Public-facing tracking portal. Minimal branding, no auth sidebar.
 * Uses glassmorphism with a lighter touch for consumer audience.
 */
import type { Metadata } from "next";
import { Inter, Space_Grotesk, JetBrains_Mono } from "next/font/google";
import "./globals.css";

const geist   = Inter({ subsets: ["latin"], variable: "--font-sans" });
const heading = Space_Grotesk({ subsets: ["latin"], variable: "--font-heading" });
const mono    = JetBrains_Mono({ subsets: ["latin"], variable: "--font-mono" });

export const metadata: Metadata = {
  title:       "Track Your Delivery — LogisticOS",
  description: "Real-time tracking for your LogisticOS delivery.",
  openGraph: {
    title:       "Track Your Delivery",
    description: "Check your delivery status in real time.",
    siteName:    "LogisticOS",
  },
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en" className={`${geist.variable} ${heading.variable} ${mono.variable} dark`}>
      <body className="bg-canvas font-sans text-white antialiased min-h-screen">
        {/* Ambient radial glow */}
        <div
          aria-hidden
          className="pointer-events-none fixed inset-0 z-0"
          style={{
            background:
              "radial-gradient(ellipse 80% 60% at 50% -10%, rgba(0,229,255,0.07) 0%, transparent 70%), " +
              "radial-gradient(ellipse 60% 40% at 80% 80%, rgba(168,85,247,0.05) 0%, transparent 60%)",
          }}
        />

        {/* Dot-matrix grid */}
        <div
          aria-hidden
          className="pointer-events-none fixed inset-0 z-0 opacity-20"
          style={{
            backgroundImage: "radial-gradient(rgba(0,229,255,0.12) 1px, transparent 1px)",
            backgroundSize:  "16px 16px",
          }}
        />

        {/* Minimal nav */}
        <nav className="relative z-10 flex items-center justify-between px-6 py-4 border-b border-white/5">
          <div className="flex items-center gap-2">
            <div
              className="h-7 w-7 rounded-lg flex items-center justify-center"
              style={{ background: "linear-gradient(135deg, #00E5FF, #A855F7)" }}
            >
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="#050810" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
                <path d="M5 18H3a2 2 0 01-2-2V8a2 2 0 012-2h3.19M15 6h2a2 2 0 012 2v8a2 2 0 01-2 2h-3.19" />
                <line x1="23" y1="13" x2="23" y2="11" />
                <polyline points="11 6 7 12 13 12 9 18" />
              </svg>
            </div>
            <span className="font-heading text-sm font-bold text-white">LogisticOS</span>
          </div>
          <span className="text-2xs font-mono text-white/30">Track · Reschedule · Feedback</span>
        </nav>

        <main className="relative z-10">{children}</main>

        <footer className="relative z-10 border-t border-white/5 py-8 text-center">
          <p className="text-2xs font-mono text-white/20">
            Powered by LogisticOS · {new Date().getFullYear()}
          </p>
        </footer>
      </body>
    </html>
  );
}
