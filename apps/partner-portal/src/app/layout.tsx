/**
 * Partner Portal — Root Layout
 * Minimal: font variables, body baseline, global CSS.
 * Dashboard chrome (sidebar, header) lives in (dashboard)/layout.tsx.
 */
import { Inter, Space_Grotesk, JetBrains_Mono, Noto_Sans_Arabic } from "next/font/google";
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
  title: "LogisticOS — Partner Portal",
  description: "Carrier performance dashboard, SLA tracking, and payout management",
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
      </NextIntlClientProvider>
      </body>
    </html>
  );
}
