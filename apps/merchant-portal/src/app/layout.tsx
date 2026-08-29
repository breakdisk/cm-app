/**
 * Merchant Portal — Root Layout
 * Minimal: font variables, body baseline, global CSS.
 * All dashboard chrome (sidebar, header) lives in (dashboard)/layout.tsx.
 * Auth pages (login) receive no sidebar — only this root wrapper.
 */
import { BrandingBoot } from "@/components/BrandingBoot";
import "./globals.css";

export const metadata = {
  title: "LogisticOS — Merchant Portal",
  description: "AI-powered last-mile delivery management",
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en" className="dark">
      <body className="bg-canvas font-sans text-white antialiased">
        {/* Animated grid overlay */}
        <div
          className="pointer-events-none fixed inset-0 z-0 bg-grid-pattern bg-grid-md opacity-[0.3]"
          aria-hidden
        />
        <BrandingBoot>{children}</BrandingBoot>
      </body>
    </html>
  );
}
