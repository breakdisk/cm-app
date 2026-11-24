import { CreditCard } from "lucide-react";
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
export default function PayoutsPage() {
  return (
    <div className="flex flex-col gap-5 p-4 sm:p-6">
      <div>
        <h1 className="font-heading text-2xl font-bold text-white flex items-center gap-2">
          <CreditCard size={20} className="text-green-signal" aria-hidden="true" />
          Payouts
        </h1>
      </div>

      <GlassCard glow="cyan">
        <div className="flex flex-col gap-2 max-w-prose" role="status">
          <h2 className="font-heading text-sm font-semibold text-white">
            Your payout statements are not available here yet
          </h2>
          <p className="text-sm text-white/60">
            This page used to show the platform account&apos;s balance, which is not yours,
            so it has been removed. Statements for your own deliveries will appear here
            once partner settlements are available.
          </p>
          <p className="text-sm text-white/60">
            For a question about a payment you are owed, contact your operations team.
          </p>
        </div>
      </GlassCard>
    </div>
  );
}
