/**
 * Merchant Portal — Root Layout
 * Minimal: font variables, body baseline, global CSS.
 * All dashboard chrome (sidebar, header) lives in (dashboard)/layout.tsx.
 * Auth pages (login) receive no sidebar — only this root wrapper.
 */
import { Inter, Space_Grotesk, JetBrains_Mono } from "next/font/google";
import { cn } from "@/lib/design-system/cn";
import "./globals.css";

const geist   = Inter({ subsets: ["latin"], variable: "--font-sans" });
const heading = Space_Grotesk({ subsets: ["latin"], variable: "--font-heading" });
const mono    = JetBrains_Mono({ subsets: ["latin"], variable: "--font-mono" });

export const metadata = {
  title: "LogisticOS — Merchant Portal",
  description: "AI-powered last-mile delivery management",
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en" className={cn(geist.variable, heading.variable, mono.variable, "dark")}>
      <body className="bg-canvas font-sans text-white antialiased">
        {/* Animated grid overlay */}
        <div
          className="pointer-events-none fixed inset-0 z-0 bg-grid-pattern bg-grid-md opacity-[0.3]"
          aria-hidden
        />
        {children}
      </body>
    </html>
  );
}
