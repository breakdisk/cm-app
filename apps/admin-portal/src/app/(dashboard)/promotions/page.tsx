"use client";
/**
 * Admin Portal — Promotions: promo codes, the loyalty ladder, company rates
 * and goodwill credit for the Move app's customers.
 *
 * Until this page every one of these was API-only. Each tab shows only to a
 * role that holds its permission: campaigns:create for codes, tiers and
 * company rates; payments:admin plus users:manage for credit, which moves
 * money-shaped value and starts from finding a customer.
 */
import { useState } from "react";
import { motion } from "framer-motion";

import { variants } from "@/lib/design-system/tokens";
import { usePermissions } from "@/hooks/usePermissions";
import { GlassCard } from "@/components/ui/glass-card";
import { CompaniesSection, CreditSection, OffersSection, TiersSection } from "./sections";

type Tab = "offers" | "tiers" | "companies" | "credit";

const TABS: { id: Tab; label: string; needs: string[] }[] = [
  { id: "offers",    label: "Promo codes",   needs: ["campaigns:create"] },
  { id: "tiers",     label: "Member tiers",  needs: ["campaigns:create"] },
  { id: "companies", label: "Company rates", needs: ["campaigns:create"] },
  { id: "credit",    label: "Account credit", needs: ["payments:admin", "users:manage"] },
];

export default function PromotionsPage() {
  const { hasPermission, loading } = usePermissions();
  const allowed = TABS.filter((t) => t.needs.every(hasPermission));
  const [picked, setPicked] = useState<Tab | null>(null);
  const tab = allowed.find((t) => t.id === picked)?.id ?? allowed[0]?.id;

  return (
    <motion.div variants={variants.fadeIn} initial="hidden" animate="visible" className="space-y-5 p-4 sm:p-6">
      <div className="min-w-0">
        <h1 className="font-heading text-lg font-semibold text-white">Promotions</h1>
        <p className="mt-1 text-sm text-white/50">What the Move app takes off a customer&apos;s price, and why.</p>
      </div>

      {!loading && allowed.length === 0 && (
        <GlassCard className="p-6 text-center">
          <p className="text-sm text-white/55">Your role can&apos;t manage promotions.</p>
        </GlassCard>
      )}

      {allowed.length > 0 && (
        <div role="tablist" aria-label="Promotions" className="-mx-4 flex gap-1 overflow-x-auto px-4 sm:mx-0 sm:px-0">
          {allowed.map((t) => (
            <button
              key={t.id}
              type="button"
              role="tab"
              aria-selected={tab === t.id}
              onClick={() => setPicked(t.id)}
              className={`shrink-0 rounded-lg px-3 py-1.5 text-xs font-medium transition-colors ${
                tab === t.id ? "bg-cyan-400/15 text-cyan-300 shadow-[0_0_12px_rgba(0,229,255,0.25)]" : "text-white/55 hover:bg-white/5"
              }`}
            >
              {t.label}
            </button>
          ))}
        </div>
      )}

      {tab === "offers" && <OffersSection />}
      {tab === "tiers" && <TiersSection />}
      {tab === "companies" && <CompaniesSection />}
      {tab === "credit" && <CreditSection />}
    </motion.div>
  );
}
