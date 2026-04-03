"use client";

import { useEffect, useRef, useState } from "react";
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

export default function LandingPage() {
  return (
    <main className="min-h-screen bg-[#050810] text-slate-100 overflow-x-hidden">
      <Navbar />
      <Hero />
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
    </main>
  );
}
