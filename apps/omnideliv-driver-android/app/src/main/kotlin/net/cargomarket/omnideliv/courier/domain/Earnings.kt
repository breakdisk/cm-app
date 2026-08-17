package net.cargomarket.omnideliv.courier.domain

/**
 * One entry as the server has it, signed exactly as stored.
 *
 * Credits positive, payouts negative, cash collected negative — so a client
 * summing the list gets the balance and cannot disagree with it. Re-deriving
 * signs on this side is how a screen starts telling a courier something the
 * payout run does not believe.
 */
data class LedgerEntry(
    val kind: String,
    val amountCents: Long,
    val externalRef: String?,
    val at: String,
)

/**
 * What the earnings screen shows.
 *
 * Confirmed and pending are two figures and are **never summed**. The payout
 * run works off the server balance; an app that quietly adds unacknowledged
 * work to it shows a number the platform does not agree with, and that
 * disagreement is how a courier stops trusting the screen.
 */
data class EarningsView(
    /** Server truth for this period. */
    val confirmedBalanceCents: Long,
    /** Locally recorded, not yet acknowledged. Displayed apart. */
    val pendingCents: Long,
    /** Milestones still unsynced, parked included. */
    val pendingCount: Int,
    /** Platform cash the courier is holding. Positive means they owe it. */
    val cashHeldCents: Long,
)

/** Kinds that represent cash moving between courier and platform. */
private val CASH_KINDS = setOf("cod_collected", "cod_remitted")

/**
 * Build the view from the server's ledger and the local queue.
 *
 * `pendingCents` counts only what the courier will *earn* — the trip and tip on
 * jobs delivered but not yet acknowledged. Cash collected on those jobs is
 * deliberately excluded: it is not earnings, and showing a courier a pending
 * figure that nets their earnings against the customer's money would reproduce,
 * on screen, the exact sign confusion the backend ledger is built to prevent.
 */
fun buildEarnings(
    entries: List<LedgerEntry>,
    unsyncedEarningsCents: Long,
    unsyncedCount: Int,
): EarningsView {
    val balance = entries.sumOf { it.amountCents }

    // Net of collected minus remitted. Negative while cash is outstanding, so
    // it is reported as a positive debt; never negative, because over-remitting
    // is representable and a negative "held" would subtract from the next debt.
    val cashNet = entries.filter { it.kind in CASH_KINDS }.sumOf { it.amountCents }
    val cashHeld = if (cashNet < 0) -cashNet else 0L

    return EarningsView(
        confirmedBalanceCents = balance,
        pendingCents = unsyncedEarningsCents,
        pendingCount = unsyncedCount,
        cashHeldCents = cashHeld,
    )
}

/**
 * Whether a payout is possible right now, and why not if it is not.
 *
 * Mirrors the server's two payout rules so the app can explain a refusal before
 * the courier taps and gets a bare failure. It is a *preview*, never an
 * authority: the server decides, and this only exists so the screen can say
 * what it is about to say.
 */
sealed interface CashoutEligibility {
    data object Eligible : CashoutEligibility
    data class HoldingCash(val cents: Long) : CashoutEligibility
    data object NothingOwed : CashoutEligibility
}

fun cashoutEligibility(view: EarningsView): CashoutEligibility = when {
    // Checked first, and this ordering is the point. A courier can be in credit
    // overall and still holding platform cash — earn 5000, collect 3000,
    // balance 2000. Paying that 2000 before the 3000 comes back leaves the
    // platform down 3000 and hands them money they already had.
    view.cashHeldCents > 0 -> CashoutEligibility.HoldingCash(view.cashHeldCents)
    view.confirmedBalanceCents <= 0 -> CashoutEligibility.NothingOwed
    else -> CashoutEligibility.Eligible
}
