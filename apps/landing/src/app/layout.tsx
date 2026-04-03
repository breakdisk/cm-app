import type { Metadata } from "next";
import "./globals.css";

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

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en" className="dark">
      <head>
        <link rel="preconnect" href="https://fonts.googleapis.com" />
        <link rel="preconnect" href="https://fonts.gstatic.com" crossOrigin="anonymous" />
        <link
          href="https://fonts.googleapis.com/css2?family=Space+Grotesk:wght@300;400;500;600;700&family=JetBrains+Mono:wght@400;500&display=swap"
          rel="stylesheet"
        />
      </head>
      <body style={{ fontFamily: "'Space Grotesk', sans-serif" }}>
        {children}
      </body>
    </html>
  );
}
