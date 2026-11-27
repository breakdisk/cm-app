package io.logisticos.driver.feature.profile.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.hilt.navigation.compose.hiltViewModel
import io.logisticos.driver.core.designsystem.*
import io.logisticos.driver.core.network.service.EarningAdjustmentItem
import io.logisticos.driver.core.network.service.EarningEntryItem
import io.logisticos.driver.core.network.service.LedgerEntryItem
import io.logisticos.driver.feature.profile.presentation.EarningsViewModel

/** "2026-06-12T08:30:00Z" / "2026-06-12" → "Jun 12" (display-only, lenient). */
private fun dayLabel(iso: String?): String {
    if (iso == null) return "—"
    val date = iso.take(10)
    return runCatching {
        java.time.LocalDate.parse(date)
            .format(java.time.format.DateTimeFormatter.ofPattern("MMM d"))
    }.getOrDefault(date)
}

/**
 * The wallet (driver design): full financial history. *Earnings* (gig only —
 * per-task contractual payouts grouped by day) and *Cash* (COD ledger: what
 * is owed to the hub, then what was remitted). Full-time drivers land directly
 * on Cash — they never see payout figures.
 *
 * @param onBack null where this is a bottom tab (no back affordance).
 */
@Composable
fun EarningsScreen(
    onBack: (() -> Unit)?,
    viewModel: EarningsViewModel = hiltViewModel(),
) {
    val state by viewModel.uiState.collectAsState()
    val c = LocalMoveColors.current
    val tabs = if (state.isGigWorker) listOf("Earnings", "Cash") else listOf("Cash")
    var tabIndex by remember { mutableIntStateOf(0) }
    if (tabIndex >= tabs.size) tabIndex = 0

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(c.ground),
    ) {
        MoveScreenHeader(
            label = "Wallet",
            title = if (state.isGigWorker) "Earnings & cash" else "Cash on hand",
            onBack = onBack,
            actions = {
                if (!state.isLoading) MoveSquareButton(Icons.Filled.Refresh, "Refresh", viewModel::refresh)
            },
        )

        when {
            state.isLoading -> Box(Modifier.fillMaxWidth().padding(top = 48.dp), contentAlignment = Alignment.Center) {
                CircularProgressIndicator(color = c.accent, strokeWidth = 2.dp)
            }
            state.error != null -> Column(
                modifier = Modifier.padding(16.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                MoveNotice(title = "Couldn't load your wallet", body = state.error!!, tone = MoveTone.Penalty)
                MoveBigButton("TRY AGAIN", onClick = viewModel::refresh, filled = false, height = 56.dp)
            }
            else -> {
                val openBalance = state.ledger?.openBalanceCents ?: 0L
                LazyColumn(
                    modifier = Modifier.fillMaxSize(),
                    contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 4.dp, bottom = 24.dp),
                ) {
                    item(key = "hero") {
                        WalletHero(
                            isGig = state.isGigWorker,
                            todayCents = state.earnings?.todayCents ?: 0L,
                            openBalanceCents = openBalance,
                        )
                    }
                    // Gig drivers see payout totals; everyone sees cash owed.
                    if (state.isGigWorker) {
                        item(key = "tiles") {
                            Row(
                                modifier = Modifier.fillMaxWidth().padding(top = 18.dp),
                                horizontalArrangement = Arrangement.spacedBy(12.dp),
                            ) {
                                MoveStatTile("This week", pesos(state.earnings?.weekCents ?: 0L), Modifier.weight(1f))
                                MoveStatTile(
                                    "Cash to remit",
                                    pesos(openBalance),
                                    Modifier.weight(1f),
                                    valueColor = if (openBalance > 0L) c.amber else null,
                                )
                            }
                        }
                    }
                    if (tabs.size > 1) {
                        item(key = "tabs") {
                            MoveSegmented(tabs, tabIndex, { tabIndex = it }, Modifier.padding(top = 22.dp))
                        }
                    }
                    when (tabs[tabIndex]) {
                        "Earnings" -> earningsItems(
                            state.earnings?.entries.orEmpty(),
                            state.earnings?.adjustments.orEmpty(),
                        )
                        else -> cashItems(
                            openEntries = state.ledger?.openEntries.orEmpty(),
                            history = state.ledger?.recentLedgers.orEmpty()
                                .filter { it.status != "open" }
                                .flatMap { it.entries },
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun WalletHero(isGig: Boolean, todayCents: Long, openBalanceCents: Long) {
    val c = LocalMoveColors.current
    val amount = pesos(if (isGig) todayCents else openBalanceCents)
    Column(Modifier.padding(top = 8.dp)) {
        Text(if (isGig) "EARNED TODAY" else "CASH TO REMIT", color = c.muted, fontSize = 12.sp, letterSpacing = 2.4.sp)
        Text(
            amount,
            color = if (!isGig && openBalanceCents > 0L) c.amber else c.ink,
            fontFamily = Condensed,
            fontWeight = FontWeight.Bold,
            // No auto-size in this Compose version: step down for long amounts
            // so seven figures still fit a 360 dp screen.
            fontSize = if (amount.length > 9) 48.sp else 68.sp,
            lineHeight = if (amount.length > 9) 52.sp else 72.sp,
            maxLines = 1,
            modifier = Modifier.padding(top = 4.dp),
        )
        Text(
            if (isGig) "Payouts for today's completed deliveries." else "Cash collected this shift, owed to the hub.",
            color = c.muted,
            fontSize = 15.sp,
            modifier = Modifier.padding(top = 6.dp),
        )
    }
}

/**
 * Per-task payout history grouped by completion day, newest first, then the
 * lines that are not payouts: pay for waiting past the free time, and fees for
 * dropped jobs. Both are already in the totals above.
 */
private fun LazyListScope.earningsItems(entries: List<EarningEntryItem>, adjustments: List<EarningAdjustmentItem>) {
    if (entries.isEmpty() && adjustments.isEmpty()) {
        item(key = "earnings-empty") { EmptyHint("No completed deliveries in this period yet.") }
        return
    }
    if (adjustments.isNotEmpty()) {
        item(key = "adjustments-h") { MoveLabel("Adjustments", Modifier.padding(top = 22.dp, bottom = 2.dp)) }
        items(adjustments, key = { "adj-${it.kind}-${it.referenceId}" }) { a ->
            val c = LocalMoveColors.current
            val credit = a.amountCents >= 0
            LedgerLine(
                title = io.logisticos.driver.feature.profile.presentation.adjustmentTitle(a.kind),
                detail = a.trackingNumber,
                monoDetail = true,
                amount = (if (credit) "+" else "−") + pesos(kotlin.math.abs(a.amountCents)),
                amountColor = if (credit) c.success else c.penalty,
            )
        }
    }
    entries.groupBy { dayLabel(it.completedAt) }.forEach { (day, dayEntries) ->
        item(key = "h-$day") {
            val c = LocalMoveColors.current
            Row(
                modifier = Modifier.fillMaxWidth().padding(top = 22.dp, bottom = 2.dp),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                MoveLabel(day)
                Text(pesos(dayEntries.sumOf { it.payoutCents ?: 0L }), color = c.success, fontSize = 14.sp, fontWeight = FontWeight.Bold)
            }
        }
        items(dayEntries, key = { it.taskId }) { e ->
            val c = LocalMoveColors.current
            LedgerLine(
                title = e.merchantName.ifBlank { "Delivery" },
                detail = e.trackingNumber,
                monoDetail = true,
                amount = e.payoutCents?.let(::pesos) ?: "—",
                amountColor = c.success,
            )
        }
    }
}

/** COD ledger: open entries first (what's owed now), then reconciled history. */
private fun LazyListScope.cashItems(openEntries: List<LedgerEntryItem>, history: List<LedgerEntryItem>) {
    if (openEntries.isEmpty() && history.isEmpty()) {
        item(key = "cash-empty") { EmptyHint("No cash movements yet — COD collections will appear here.") }
        return
    }
    if (openEntries.isNotEmpty()) {
        item(key = "open-h") {
            MoveLabel("This shift · not yet remitted", Modifier.padding(top = 22.dp, bottom = 2.dp), dot = LocalMoveColors.current.amber)
        }
        items(openEntries, key = { "o-${it.id}" }) { LedgerRow(it) }
    }
    if (history.isNotEmpty()) {
        item(key = "hist-h") {
            MoveLabel("Settled history", Modifier.padding(top = 22.dp, bottom = 2.dp))
        }
        items(history, key = { "s-${it.id}" }) { LedgerRow(it) }
    }
}

@Composable
private fun LedgerRow(e: LedgerEntryItem) {
    val c = LocalMoveColors.current
    val isDebit = e.amountCents > 0
    val (label, color) = when (e.kind) {
        "pickup_debit" -> "Pickup cash collected" to c.amber
        "cod_debit"    -> "COD collected" to c.amber
        else           -> "Remitted to hub" to c.success
    }
    LedgerLine(
        title = label,
        detail = dayLabel(e.createdAt),
        amount = (if (isDebit) "+" else "−") + pesos(kotlin.math.abs(e.amountCents)),
        amountColor = color,
    )
}

@Composable
private fun LedgerLine(title: String, detail: String?, amount: String, amountColor: Color, monoDetail: Boolean = false) {
    val c = LocalMoveColors.current
    Column {
        Row(
            modifier = Modifier.fillMaxWidth().padding(vertical = 16.dp),
            horizontalArrangement = Arrangement.spacedBy(14.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(Modifier.weight(1f)) {
                Text(title, color = c.ink, fontSize = 16.sp, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                detail?.let {
                    Text(it, color = c.muted, fontSize = 13.sp, fontFamily = if (monoDetail) FontFamily.Monospace else null, modifier = Modifier.padding(top = 2.dp))
                }
            }
            Text(amount, color = amountColor, fontFamily = Condensed, fontWeight = FontWeight.Bold, fontSize = 24.sp, maxLines = 1)
        }
        MoveDivider()
    }
}

@Composable
private fun EmptyHint(text: String) {
    MoveNotice(
        title = "Nothing here yet",
        body = text,
        tone = MoveTone.Neutral,
        modifier = Modifier.padding(top = 16.dp),
    )
}
