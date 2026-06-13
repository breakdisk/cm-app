/**
 * Customer Portal — Root Layout
 * Public-facing tracking portal. Minimal branding, no auth sidebar.
 * Uses glassmorphism with a lighter touch for consumer audience.
 */
import type { Metadata } from "next";
import { Inter, Space_Grotesk, JetBrains_Mono } from "next/font/google";
import { BrandingBoot } from "@/components/BrandingBoot";
import { BrandedNav, BrandedFooter } from "@/components/BrandedChrome";
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

        <BrandingBoot>
          {/* Minimal nav — brand-aware */}
          <BrandedNav />

          <main className="relative z-10">{children}</main>

          <BrandedFooter />
        </BrandingBoot>
      </body>
    </html>
  );
}
