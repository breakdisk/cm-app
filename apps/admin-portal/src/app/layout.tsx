/**
 * Admin / Operations Portal — Root Layout
 * Minimal: font variables, body baseline, global CSS.
 * Dashboard chrome (sidebar, header) lives in (dashboard)/layout.tsx.
 */
import { Inter, Space_Grotesk, JetBrains_Mono, Noto_Sans_Arabic } from "next/font/google";
import { Toaster } from "sonner";
import { cn } from "@/lib/design-system/cn";
import { BrandingBoot } from "@/components/BrandingBoot";
import { NextIntlClientProvider } from "next-intl";
import { getLocale, getMessages } from "next-intl/server";
import "./globals.css";

const geist   = Inter({ subsets: ["latin"], variable: "--font-sans" });
const heading = Space_Grotesk({ subsets: ["latin"], variable: "--font-heading" });
const mono    = JetBrains_Mono({ subsets: ["latin"], variable: "--font-mono" });
const arabic  = Noto_Sans_Arabic({ subsets: ["arabic"], variable: "--font-arabic" });

export const metadata = {
  title: "LogisticOS — Operations Portal",
  description: "Real-time dispatch console and operations management",
};

export default async function RootLayout({ children }: { children: React.ReactNode }) {
  const locale = await getLocale();
  const messages = await getMessages();
  return (
    <html lang={locale} dir={locale === "ar" ? "rtl" : "ltr"} className={cn(geist.variable, heading.variable, mono.variable, arabic.variable, "dark")}>
      <body className="bg-canvas font-sans text-white antialiased">
        <NextIntlClientProvider locale={locale} messages={messages}>
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
      </NextIntlClientProvider>
      </body>
    </html>
  );
}
