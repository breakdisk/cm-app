package net.cargomarket.omnideliv.courier.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import net.cargomarket.omnideliv.courier.data.CourierApi
import net.cargomarket.omnideliv.courier.data.OutboundRepository
import net.cargomarket.omnideliv.courier.domain.CashoutEligibility
import net.cargomarket.omnideliv.courier.domain.EarningsView
import net.cargomarket.omnideliv.courier.domain.LedgerEntry
import net.cargomarket.omnideliv.courier.domain.buildEarnings
import net.cargomarket.omnideliv.courier.domain.cashoutEligibility
import javax.inject.Inject

data class EarningsUiState(
    val view: EarningsView? = null,
    val entries: List<LedgerEntry> = emptyList(),
    val period: String = "",
    val failed: Boolean = false,
)

@HiltViewModel
class EarningsViewModel @Inject constructor(
    private val api: CourierApi,
    outbound: OutboundRepository,
) : ViewModel() {

    private val _raw = MutableStateFlow(EarningsUiState())

    /**
     * Server truth combined with the local queue depth.
     *
     * The pending *count* is real; the pending *amount* is deliberately zero.
     * The app does not know what an unsynced delivery pays — `trip_cents` is
     * declared per assignment by the product, and guessing it here would put a
     * number on screen the platform never agreed to.
     */
    val state: StateFlow<EarningsUiState> =
        combine(_raw, outbound.pendingCount) { s, pending ->
            val entries = s.entries
            s.copy(view = buildEarnings(entries, unsyncedEarningsCents = 0, unsyncedCount = pending))
        }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), EarningsUiState())

    fun load() {
        viewModelScope.launch {
            val result = runCatching { api.earnings() }
            result.fold(
                onSuccess = { res ->
                    val body = res.body()
                    _raw.value = if (res.isSuccessful && body != null) {
                        EarningsUiState(
                            entries = body.entries.map {
                                LedgerEntry(it.kind, it.amountCents, it.externalRef, it.at)
                            },
                            period = body.period,
                            failed = false,
                        )
                    } else {
                        _raw.value.copy(failed = true)
                    }
                },
                onFailure = { _raw.value = _raw.value.copy(failed = true) },
            )
        }
    }
}

/**
 * What the courier has earned, and what they are holding.
 *
 * Read-only. There is no cash-out button because the payout rail refuses any
 * courier holding platform cash, and a button that is disabled most of a shift
 * teaches people to ignore it. The eligibility line says the same thing in words
 * that name what to do about it.
 */
@Composable
fun EarningsScreen(vm: EarningsViewModel = hiltViewModel()) {
    val state by vm.state.collectAsState()
    LaunchedEffect(Unit) { vm.load() }

    Column(
        Modifier
            .fillMaxSize()
            .background(Tokens.Base)
            .verticalScroll(rememberScrollState())
            .padding(16.dp),
    ) {
        Text("Earnings", color = Tokens.Text, fontWeight = FontWeight.Bold, fontSize = 22.sp)
        if (state.period.isNotBlank()) {
            Text(state.period, color = Tokens.TextMuted, fontSize = 12.sp)
        }
        if (state.failed) {
            Spacer(Modifier.height(6.dp))
            Text("Could not refresh — showing what we last loaded", color = Tokens.Amber, fontSize = 12.sp)
        }

        val view = state.view
        Spacer(Modifier.height(18.dp))

        if (view == null) {
            Text("Loading…", color = Tokens.TextMuted, fontSize = 14.sp)
            return@Column
        }

        // Confirmed and pending never share a figure. The payout run works off
        // the server balance, and an app that quietly adds unacknowledged work
        // to it shows a number the platform does not agree with — which is how
        // a courier stops trusting every number in the app.
        Figure(
            label = "Confirmed",
            value = pesos(view.confirmedBalanceCents),
            color = if (view.confirmedBalanceCents < 0) Tokens.Amber else Tokens.Signal,
            note = "What the platform agrees it owes you",
        )

        if (view.pendingCount > 0) {
            Spacer(Modifier.height(10.dp))
            Figure(
                label = "Waiting to sync",
                value = "${view.pendingCount} " +
                    if (view.pendingCount == 1) "delivery" else "deliveries",
                color = Tokens.TextMuted,
                // Says plainly that the amount is unknown rather than showing a
                // guess. The app is not told what an unsynced job pays.
                note = "Not counted above until the server confirms them",
            )
        }

        if (view.cashHeldCents > 0) {
            Spacer(Modifier.height(10.dp))
            Figure(
                label = "Cash you are holding",
                value = pesos(view.cashHeldCents),
                color = Tokens.Amber,
                note = "The customer's money, collected on delivery. Remit it to your hub.",
            )
        }

        Spacer(Modifier.height(20.dp))
        CashoutLine(cashoutEligibility(view))

        if (state.entries.isNotEmpty()) {
            Spacer(Modifier.height(22.dp))
            Text("This period", color = Tokens.TextMuted, fontSize = 12.sp, fontWeight = FontWeight.Bold)
            Spacer(Modifier.height(8.dp))
            state.entries.forEach { EntryRow(it) }
        }
    }
}

@Composable
private fun Figure(label: String, value: String, color: androidx.compose.ui.graphics.Color, note: String) {
    Column(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(12.dp))
            .background(Tokens.Surface)
            .border(1.dp, Tokens.Border, RoundedCornerShape(12.dp))
            .padding(14.dp),
    ) {
        Text(label, color = Tokens.TextMuted, fontSize = 12.sp, fontWeight = FontWeight.Bold)
        Spacer(Modifier.height(4.dp))
        Text(
            value,
            color = color,
            fontSize = 28.sp,
            fontWeight = FontWeight.Bold,
            fontFamily = FontFamily.Monospace,
        )
        Spacer(Modifier.height(4.dp))
        Text(note, color = Tokens.TextMuted, fontSize = 11.sp, lineHeight = 15.sp)
    }
}

@Composable
private fun CashoutLine(eligibility: CashoutEligibility) {
    val (text, color) = when (eligibility) {
        is CashoutEligibility.HoldingCash ->
            "Remit ${pesos(eligibility.cents)} to your hub before you can be paid out." to Tokens.Amber

        CashoutEligibility.NothingOwed ->
            "Nothing owed to you right now." to Tokens.TextMuted

        CashoutEligibility.Eligible ->
            "You are due a payout in the next run." to Tokens.Signal
    }
    Text(text, color = color, fontSize = 13.sp, lineHeight = 18.sp)
}

@Composable
private fun EntryRow(entry: LedgerEntry) {
    Row(
        Modifier
            .fillMaxWidth()
            .padding(vertical = 7.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.padding(end = 12.dp)) {
            Text(entryLabel(entry.kind), color = Tokens.Text, fontSize = 13.sp)
            Text(entry.at.take(10), color = Tokens.TextMuted, fontSize = 10.sp)
        }
        Text(
            // Carries the stored sign. A client that re-derived it could
            // disagree with the ledger it is displaying.
            text = pesos(entry.amountCents),
            color = if (entry.amountCents < 0) Tokens.Amber else Tokens.Signal,
            fontSize = 14.sp,
            fontWeight = FontWeight.Bold,
            fontFamily = FontFamily.Monospace,
        )
    }
}

private fun entryLabel(kind: String) = when (kind) {
    "trip_earning" -> "Trip"
    "tip" -> "Tip"
    "cod_collected" -> "Cash collected"
    "cod_remitted" -> "Cash remitted"
    "payout" -> "Paid out"
    "adjustment" -> "Adjustment"
    else -> kind
}
