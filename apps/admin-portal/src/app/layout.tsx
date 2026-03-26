/**
 * Admin / Operations Portal — Root Layout
 * Minimal: font variables, body baseline, global CSS.
 * Dashboard chrome (sidebar, header) lives in (dashboard)/layout.tsx.
 */
import { Inter, Space_Grotesk, JetBrains_Mono } from "next/font/google";
import { cn } from "@/lib/design-system/cn";
import "./globals.css";

const geist   = Inter({ subsets: ["latin"], variable: "--font-sans" });
const heading = Space_Grotesk({ subsets: ["latin"], variable: "--font-heading" });
const mono    = JetBrains_Mono({ subsets: ["latin"], variable: "--font-mono" });

export const metadata = {
  title: "LogisticOS — Operations Portal",
  description: "Real-time dispatch console and operations management",
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en" className={cn(geist.variable, heading.variable, mono.variable, "dark")}>
      <body className="bg-canvas font-sans text-white antialiased">
        <div
          className="pointer-events-none fixed inset-0 z-0 bg-grid-pattern bg-grid-md opacity-[0.3]"
          aria-hidden
        />
        {children}
      </body>
    </html>
  );
}
