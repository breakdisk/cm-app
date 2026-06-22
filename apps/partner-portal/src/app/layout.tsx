/**
 * Partner Portal — Root Layout
 * Minimal: font variables, body baseline, global CSS.
 * Dashboard chrome (sidebar, header) lives in (dashboard)/layout.tsx.
 */
import { BrandingBoot } from "@/components/BrandingBoot";
import "./globals.css";

export const metadata = {
  title: "LogisticOS — Partner Portal",
  description: "Carrier performance dashboard, SLA tracking, and payout management",
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en" className="dark">
      <body className="bg-canvas font-sans text-white antialiased">
        <div
          className="pointer-events-none fixed inset-0 z-0 bg-grid-pattern bg-grid-md opacity-[0.3]"
          aria-hidden
        />
        <BrandingBoot>{children}</BrandingBoot>
      </body>
    </html>
  );
}
