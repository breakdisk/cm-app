import type { Metadata } from "next";
import { Noto_Sans_Arabic } from "next/font/google";
import { NextIntlClientProvider } from "next-intl";
import { getLocale, getMessages } from "next-intl/server";
import "./globals.css";

const arabic  = Noto_Sans_Arabic({ subsets: ["arabic"], variable: "--font-arabic" });


export const metadata: Metadata = {
  title: "CargoMarket — AI Logistics Platform",
  description:
    "One platform for Small, Medium & Enterprise logistics. AI-powered dispatch, real-time tracking, and automated customer engagement.",
  keywords: ["logistics", "AI", "shipping", "cargo", "last-mile delivery", "SaaS"],
  openGraph: {
    title: "CargoMarket — AI Logistics Platform",
    description: "Integrating small, medium & enterprise businesses through one AI logistics platform.",
    type: "website",
  },
};

export default async function RootLayout({ children }: { children: React.ReactNode }) {
  const locale = await getLocale();
  const messages = await getMessages();
  return (
    <html lang={locale} dir={locale === "ar" ? "rtl" : "ltr"} className={`dark ${arabic.variable}`}>
      <head>
        <link rel="preconnect" href="https://fonts.googleapis.com" />
        <link rel="preconnect" href="https://fonts.gstatic.com" crossOrigin="anonymous" />
        <link
          href="https://fonts.googleapis.com/css2?family=Space+Grotesk:wght@300;400;500;600;700&family=JetBrains+Mono:wght@400;500&display=swap"
          rel="stylesheet"
        />
      </head>
      <body style={{ fontFamily: "'Space Grotesk', sans-serif" }}>
        <NextIntlClientProvider locale={locale} messages={messages}>
          {children}
        </NextIntlClientProvider>
      </body>
    </html>
  );
}
