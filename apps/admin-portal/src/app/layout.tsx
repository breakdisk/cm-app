/**
 * Admin / Operations Portal — Root Layout
 * Minimal: font variables, body baseline, global CSS.
 * Dashboard chrome (sidebar, header) lives in (dashboard)/layout.tsx.
 */
import { Toaster } from "sonner";
import { BrandingBoot } from "@/components/BrandingBoot";
import "./globals.css";

export const metadata = {
  title: "LogisticOS — Operations Portal",
  description: "Real-time dispatch console and operations management",
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
        <Toaster
          position="bottom-right"
          theme="dark"
          toastOptions={{
            style: {
              background: "rgba(13,20,34,0.95)",
              border: "1px solid rgba(255,255,255,0.08)",
              color: "#fff",
              backdropFilter: "blur(12px)",
            },
          }}
        />
      </body>
    </html>
  );
}
