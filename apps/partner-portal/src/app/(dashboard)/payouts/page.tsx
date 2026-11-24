import { CreditCard } from "lucide-react";
import { getTranslations } from "next-intl/server";
import { GlassCard } from "@/components/ui/glass-card";

/**
 * Payouts.
 *
 * This page used to show the platform account's wallet, invoices and a
 * withdrawal form. That is the tenant's own money, not the partner's, so a
 * partner token can no longer read it and the page was emptied rather than left
 * showing figures that were never theirs. Statements for a partner's own
 * deliveries will replace this once partner settlements exist.
 */
export default async function PayoutsPage() {
  const t = await getTranslations("payouts");
  return (
    <div className="flex flex-col gap-5 p-4 sm:p-6">
      <div>
        <h1 className="font-heading text-2xl font-bold text-white flex items-center gap-2">
          <CreditCard size={20} className="text-green-signal" aria-hidden="true" />
          {t("title")}
        </h1>
      </div>

      <GlassCard glow="cyan">
        <div className="flex flex-col gap-2 max-w-prose" role="status">
          <h2 className="font-heading text-sm font-semibold text-white">
            {t("emptyHeading")}
          </h2>
          <p className="text-sm text-white/60">
            {t("emptyBody1")}
          </p>
          <p className="text-sm text-white/60">
            {t("emptyBody2")}
          </p>
        </div>
      </GlassCard>
    </div>
  );
}
