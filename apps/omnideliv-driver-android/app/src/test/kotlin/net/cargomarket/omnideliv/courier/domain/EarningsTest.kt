package net.cargomarket.omnideliv.courier.domain

import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertInstanceOf
import org.junit.jupiter.api.Test

class EarningsTest {

    private fun e(kind: String, cents: Long) = LedgerEntry(kind, cents, "job", "2026-08-17T10:00:00Z")

    /**
     * The sign convention, from the client's side. A courier who earned ₱35 and
     * is holding ₱389 of platform cash is ₱354 in debt, not ₱424 in credit.
     */
    @Test
    fun `holding cash puts the balance in debt`() {
        val v = buildEarnings(
            entries = listOf(e("trip_earning", 3_500), e("cod_collected", -38_900)),
            unsyncedEarningsCents = 0,
            unsyncedCount = 0,
        )
        assertEquals(3_500L - 38_900L, v.confirmedBalanceCents)
        assertEquals(38_900L, v.cashHeldCents)
    }

    @Test
    fun `remitting clears the debt and leaves the earning`() {
        val v = buildEarnings(
            entries = listOf(
                e("trip_earning", 3_500),
                e("cod_collected", -38_900),
                e("cod_remitted", 38_900),
            ),
            unsyncedEarningsCents = 0,
            unsyncedCount = 0,
        )
        assertEquals(0L, v.cashHeldCents)
        assertEquals(3_500L, v.confirmedBalanceCents)
    }

    /** Over-remitting happens. A negative "held" would subtract from the next debt. */
    @Test
    fun `over-remitting reports no cash held rather than a negative`() {
        val v = buildEarnings(
            entries = listOf(e("cod_collected", -10_000), e("cod_remitted", 15_000)),
            unsyncedEarningsCents = 0,
            unsyncedCount = 0,
        )
        assertEquals(0L, v.cashHeldCents)
    }

    /** Counting earnings would make a well-paid courier look like they owed less. */
    @Test
    fun `earnings do not offset cash held`() {
        val v = buildEarnings(
            entries = listOf(
                e("trip_earning", 50_000),
                e("tip", 5_000),
                e("cod_collected", -10_000),
            ),
            unsyncedEarningsCents = 0,
            unsyncedCount = 0,
        )
        assertEquals(10_000L, v.cashHeldCents)
        assertEquals(45_000L, v.confirmedBalanceCents)
    }

    /**
     * The decision this screen is built on. The payout run works off the server
     * balance; an app that adds unacknowledged work to it shows a number the
     * platform does not agree with.
     */
    @Test
    fun `confirmed and pending are separate figures`() {
        val v = buildEarnings(
            entries = listOf(e("trip_earning", 3_500)),
            unsyncedEarningsCents = 4_200,
            unsyncedCount = 1,
        )
        assertEquals(3_500L, v.confirmedBalanceCents)
        assertEquals(4_200L, v.pendingCents)
        assertEquals(1, v.pendingCount)
        // Deliberately no `total` on EarningsView: the type cannot express the
        // sum, so no screen can render one by accident.
    }

    /**
     * The ordering inside `cashoutEligibility` is the whole rule. A courier can
     * be in credit overall and still holding platform cash — earn 5000, collect
     * 3000, balance 2000. Paying that 2000 before the 3000 comes back leaves
     * the platform down 3000 and hands them money they already had.
     */
    @Test
    fun `a courier in credit but holding cash cannot cash out`() {
        val v = buildEarnings(
            entries = listOf(e("trip_earning", 5_000), e("cod_collected", -3_000)),
            unsyncedEarningsCents = 0,
            unsyncedCount = 0,
        )
        assertEquals(2_000L, v.confirmedBalanceCents)
        assertEquals(3_000L, v.cashHeldCents)

        val r = cashoutEligibility(v)
        assertInstanceOf(CashoutEligibility.HoldingCash::class.java, r)
        assertEquals(3_000L, (r as CashoutEligibility.HoldingCash).cents)
    }

    @Test
    fun `a clean positive balance is eligible`() {
        val v = buildEarnings(listOf(e("trip_earning", 3_500)), 0, 0)
        assertInstanceOf(CashoutEligibility.Eligible::class.java, cashoutEligibility(v))
    }

    /** A negative balance means the courier owes us; "paying" it moves money the wrong way. */
    @Test
    fun `nothing owed is not eligible`() {
        val zero = buildEarnings(emptyList(), 0, 0)
        assertInstanceOf(CashoutEligibility.NothingOwed::class.java, cashoutEligibility(zero))

        val negative = buildEarnings(listOf(e("adjustment", -500)), 0, 0)
        assertInstanceOf(CashoutEligibility.NothingOwed::class.java, cashoutEligibility(negative))
    }

    /** A courier mid-shift and one who has sold nothing both have earnings, and they are nil. */
    @Test
    fun `an empty ledger is a zero, not an error`() {
        val v = buildEarnings(emptyList(), 0, 0)
        assertEquals(0L, v.confirmedBalanceCents)
        assertEquals(0L, v.cashHeldCents)
    }
}
