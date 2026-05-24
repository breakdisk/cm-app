"use client";

import { useState } from "react";
import Navbar from "@/components/Navbar";
import Hero from "@/components/Hero";
import LogoTicker from "@/components/LogoTicker";
import PlatformTiers from "@/components/PlatformTiers";
import Features from "@/components/Features";
import HowItWorks from "@/components/HowItWorks";
import AISection from "@/components/AISection";
import Metrics from "@/components/Metrics";
import Pricing from "@/components/Pricing";
import Testimonials from "@/components/Testimonials";
import CTA from "@/components/CTA";
import Footer from "@/components/Footer";
import QuoteTool from "@/components/QuoteTool";

export default function LandingPage() {
  const [quoteOpen, setQuoteOpen] = useState(false);

  return (
    <main className="min-h-screen bg-[#050810] text-slate-100 overflow-x-hidden">
      <Navbar />
      <Hero onGetQuote={() => setQuoteOpen(true)} />
      <LogoTicker />
      <PlatformTiers />
      <Features />
      <HowItWorks />
      <AISection />
      <Metrics />
      <Pricing />
      <Testimonials />
      <CTA />
      <Footer />

      {/* Global quote modal — rendered outside section flow so it overlays everything */}
      <QuoteTool isOpen={quoteOpen} onClose={() => setQuoteOpen(false)} />
    </main>
  );
}
