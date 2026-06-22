package io.logisticos.driver.feature.profile.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.hilt.navigation.compose.hiltViewModel
import io.logisticos.driver.core.network.service.EarningEntryItem
import io.logisticos.driver.core.network.service.LedgerEntryItem
import io.logisticos.driver.feature.profile.presentation.EarningsViewModel

private val Canvas0 = Color(0xFF050810)
private val Cyan    = Color(0xFF00E5FF)
private val Green   = Color(0xFF00FF88)
private val Amber   = Color(0xFFFFAB00)
private val Red     = Color(0xFFFF4D4D)
private val Glass   = Color(0x0AFFFFFF)
private val Border  = Color(0x14FFFFFF)

private fun peso(cents: Long): String = "₱${"%,.2f".format(cents / 100.0)}"

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
 * Full financial history: *Earnings* (gig only — per-task contractual payouts
 * grouped by day) and *Cash* (COD ledger: debits red, remittances green).
 * Full-time drivers land directly on Cash — they never see payout figures.
 */
@Composable
fun EarningsScreen(
    onBack: () -> Unit,
    viewModel: EarningsViewModel = hiltViewModel(),
) {
    val state by viewModel.uiState.collectAsState()
    val tabs = if (state.isGigWorker) listOf("Earnings", "Cash") else listOf("Cash")
    var tabIndex by remember { mutableIntStateOf(0) }
    if (tabIndex >= tabs.size) tabIndex = 0

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(Canvas0)
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        // Header
        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.SpaceBetween,
        ) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                IconButton(onClick = onBack) {
                    Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back", tint = Color.White)
                }
                Text("Earnings & Cash", color = Color.White, fontSize = 20.sp, fontWeight = FontWeight.Bold)
            }
            IconButton(onClick = viewModel::refresh, enabled = !state.isLoading) {
                Icon(
                    Icons.Default.Refresh,
                    contentDescription = "Refresh",
                    tint = Color.White.copy(alpha = if (!state.isLoading) 0.6f else 0.2f),
                )
            }
        }

        if (state.isLoading) {
            Box(Modifier.fillMaxWidth().padding(top = 48.dp), contentAlignment = Alignment.Center) {
                CircularProgressIndicator(color = Cyan, strokeWidth = 2.dp)
            }
            return@Column
        }
        state.error?.let { err ->
            Text(err, color = Red, fontSize = 13.sp)
            return@Column
        }

        // Summary header — gig drivers see payout totals; everyone sees cash owed.
        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            if (state.isGigWorker) {
                SummaryCell("Today", peso(state.earnings?.todayCents ?: 0L), Green, Modifier.weight(1f))
                SummaryCell("This week", peso(state.earnings?.weekCents ?: 0L), Cyan, Modifier.weight(1f))
            }
            SummaryCell(
                "Cash to remit",
                peso(state.ledger?.openBalanceCents ?: 0L),
                if ((state.ledger?.openBalanceCents ?: 0L) > 0L) Amber else Color.White.copy(alpha = 0.6f),
                Modifier.weight(1f),
            )
        }

        if (tabs.size > 1) {
            TabRow(
                selectedTabIndex = tabIndex,
                containerColor = Color.Transparent,
                contentColor = Cyan,
            ) {
                tabs.forEachIndexed { i, label ->
                    Tab(
                        selected = tabIndex == i,
                        onClick = { tabIndex = i },
                        text = {
                            Text(
                                label,
                                color = if (tabIndex == i) Cyan else Color.White.copy(alpha = 0.5f),
                                fontWeight = FontWeight.SemiBold,
                            )
                        },
                    )
                }
            }
        }

        when (tabs[tabIndex]) {
            "Earnings" -> EarningsList(state.earnings?.entries.orEmpty())
            else       -> CashList(
                openEntries = state.ledger?.openEntries.orEmpty(),
                history = state.ledger?.recentLedgers.orEmpty()
                    .filter { it.status != "open" }
                    .flatMap { it.entries },
            )
        }
    }
}

@Composable
private fun SummaryCell(label: String, value: String, color: Color, modifier: Modifier = Modifier) {
    Column(
        modifier = modifier
            .clip(RoundedCornerShape(14.dp))
            .background(Glass)
            .padding(horizontal = 14.dp, vertical = 12.dp),
        verticalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        Text(value, color = color, fontSize = 17.sp, fontWeight = FontWeight.Bold, fontFamily = FontFamily.Monospace)
        Text(label, color = Color.White.copy(alpha = 0.45f), fontSize = 11.sp)
    }
}

/** Per-task payout history grouped by completion day, newest first. */
@Composable
private fun EarningsList(entries: List<EarningEntryItem>) {
    if (entries.isEmpty()) {
        EmptyHint("No completed deliveries in this period yet.")
        return
    }
    val grouped = entries.groupBy { dayLabel(it.completedAt) }
    LazyColumn(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        grouped.forEach { (day, dayEntries) ->
            item(key = "h-$day") {
                Row(
                    modifier = Modifier.fillMaxWidth().padding(top = 6.dp),
                    horizontalArrangement = Arrangement.SpaceBetween,
                ) {
                    Text(day, color = Color.White.copy(alpha = 0.5f), fontSize = 12.sp, fontWeight = FontWeight.Bold)
                    Text(
                        peso(dayEntries.sumOf { it.payoutCents ?: 0L }),
                        color = Green.copy(alpha = 0.8f), fontSize = 12.sp,
                        fontWeight = FontWeight.Bold, fontFamily = FontFamily.Monospace,
                    )
                }
            }
            items(dayEntries, key = { it.taskId }) { e ->
                Card(
                    colors = CardDefaults.cardColors(containerColor = Glass),
                    border = BorderStroke(1.dp, Border),
                ) {
                    Row(
                        modifier = Modifier.fillMaxWidth().padding(horizontal = 14.dp, vertical = 10.dp),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Column(Modifier.weight(1f)) {
                            Text(
                                e.merchantName.ifBlank { "Delivery" },
                                color = Color.White, fontSize = 14.sp, fontWeight = FontWeight.SemiBold,
                            )
                            e.trackingNumber?.let {
                                Text(it, color = Cyan.copy(alpha = 0.65f), fontSize = 11.sp, fontFamily = FontFamily.Monospace)
                            }
                        }
                        Text(
                            e.payoutCents?.let(::peso) ?: "—",
                            color = Green, fontSize = 14.sp,
                            fontWeight = FontWeight.Bold, fontFamily = FontFamily.Monospace,
                        )
                    }
                }
            }
        }
    }
}

/** COD ledger: open entries first (what's owed now), then reconciled history. */
@Composable
private fun CashList(openEntries: List<LedgerEntryItem>, history: List<LedgerEntryItem>) {
    if (openEntries.isEmpty() && history.isEmpty()) {
        EmptyHint("No cash movements yet — COD collections will appear here.")
        return
    }
    LazyColumn(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        if (openEntries.isNotEmpty()) {
            item(key = "open-h") {
                Text(
                    "CURRENT SHIFT — UNREMITTED", color = Amber, fontSize = 11.sp,
                    fontWeight = FontWeight.Bold, letterSpacing = 1.5.sp,
                    modifier = Modifier.padding(top = 6.dp),
                )
            }
            items(openEntries, key = { "o-${it.id}" }) { LedgerRow(it) }
        }
        if (history.isNotEmpty()) {
            item(key = "hist-h") {
                Text(
                    "SETTLED HISTORY", color = Color.White.copy(alpha = 0.4f), fontSize = 11.sp,
                    fontWeight = FontWeight.Bold, letterSpacing = 1.5.sp,
                    modifier = Modifier.padding(top = 10.dp),
                )
            }
            items(history, key = { "s-${it.id}" }) { LedgerRow(it) }
        }
    }
}

@Composable
private fun LedgerRow(e: LedgerEntryItem) {
    val isDebit = e.amountCents > 0
    val (label, color) = when (e.kind) {
        "pickup_debit" -> "Pickup cash collected" to Red
        "cod_debit"    -> "COD collected" to Red
        else           -> "Remitted to hub" to Green
    }
    Card(
        colors = CardDefaults.cardColors(containerColor = Glass),
        border = BorderStroke(1.dp, Border),
    ) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(horizontal = 14.dp, vertical = 10.dp),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(Modifier.weight(1f)) {
                Text(label, color = Color.White, fontSize = 13.sp, fontWeight = FontWeight.Medium)
                Text(dayLabel(e.createdAt), color = Color.White.copy(alpha = 0.4f), fontSize = 11.sp)
            }
            Text(
                (if (isDebit) "+" else "−") + peso(kotlin.math.abs(e.amountCents)),
                color = color, fontSize = 14.sp,
                fontWeight = FontWeight.Bold, fontFamily = FontFamily.Monospace,
            )
        }
    }
}

@Composable
private fun EmptyHint(text: String) {
    Box(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(14.dp))
            .background(Glass)
            .padding(24.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(text, color = Color.White.copy(alpha = 0.5f), fontSize = 13.sp)
    }
}
