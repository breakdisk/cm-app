package net.cargomarket.omnideliv.courier.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
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
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.hilt.navigation.compose.hiltViewModel
import net.cargomarket.omnideliv.courier.domain.OfferCard

/**
 * On duty, and what is on offer.
 *
 * The step between signing in and having any work. Offers expire, so this polls
 * rather than waiting for a push — and only while the courier is actually on
 * duty, so opening the app to check earnings does not spend their battery.
 */
@Composable
fun ShiftScreen(
    vm: ShiftViewModel = hiltViewModel(),
    onClaimed: (String) -> Unit = {},
) {
    val state by vm.state.collectAsState()

    // Navigation is a side effect of state, not of the tap. A tap that claimed
    // successfully but was followed by a config change would otherwise lose the
    // navigation and strand the courier on an empty offer list holding a job.
    LaunchedEffect(state) {
        (state as? ShiftState.Claimed)?.let { onClaimed(it.externalRef) }
    }

    Column(Modifier.fillMaxSize().background(Tokens.Base)) {
        DutyBar(
            online = state !is ShiftState.Offline,
            stale = (state as? ShiftState.Online)?.stale == true,
            onToggle = { on -> if (on) vm.goOnline() else vm.goOffline() },
        )

        when (val s = state) {
            is ShiftState.Offline -> Centered(
                "You are off duty",
                "Go on duty to start receiving offers.",
            )

            is ShiftState.Online -> {
                s.notice?.let { NoticeLine(it) }
                if (s.offers.isEmpty()) {
                    Centered(
                        "No offers right now",
                        "You will see jobs here as they come in.",
                    )
                } else {
                    LazyColumn(
                        Modifier.fillMaxSize().padding(horizontal = 12.dp),
                        verticalArrangement = Arrangement.spacedBy(10.dp),
                    ) {
                        items(s.offers, key = { it.assignmentId }) { offer ->
                            OfferCardView(
                                offer = offer,
                                busy = s.claiming == offer.assignmentId,
                                anyClaiming = s.claiming != null,
                                onAccept = { vm.claim(offer.assignmentId) },
                            )
                        }
                    }
                }
            }

            // Rendered for the instant between claiming and the host navigating.
            is ShiftState.Claimed -> Centered("Job taken", "Opening your route…")
        }
    }
}

@Composable
private fun DutyBar(online: Boolean, stale: Boolean, onToggle: (Boolean) -> Unit) {
    Column(
        Modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp, vertical = 12.dp),
    ) {
        Row(
            Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column {
                Text(
                    text = if (online) "On duty" else "Off duty",
                    color = if (online) Tokens.Signal else Tokens.TextMuted,
                    fontWeight = FontWeight.Bold,
                    fontSize = 19.sp,
                )
                Text(
                    text = if (online) "Watching for offers" else "Not receiving offers",
                    color = Tokens.TextMuted,
                    fontSize = 12.sp,
                )
            }
            Switch(
                checked = online,
                onCheckedChange = onToggle,
                colors = SwitchDefaults.colors(
                    checkedThumbColor = Tokens.SignalInk,
                    checkedTrackColor = Tokens.Signal,
                    uncheckedThumbColor = Tokens.TextMuted,
                    uncheckedTrackColor = Tokens.Surface,
                ),
            )
        }

        // Honest about a failed poll rather than showing a frozen list as live.
        if (stale) {
            Spacer(Modifier.height(8.dp))
            Text(
                "No signal — this list may be out of date",
                color = Tokens.Amber,
                fontSize = 12.sp,
            )
        }
    }
}

@Composable
private fun OfferCardView(
    offer: OfferRow,
    busy: Boolean,
    anyClaiming: Boolean,
    onAccept: () -> Unit,
) {
    Column(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(12.dp))
            .background(Tokens.Surface)
            .border(1.dp, Tokens.Border, RoundedCornerShape(12.dp))
            .padding(14.dp),
    ) {
        Row(
            Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.Top,
        ) {
            Text(
                text = pesos(offer.earningsCents()),
                color = Tokens.Signal,
                fontWeight = FontWeight.Bold,
                fontSize = 26.sp,
                fontFamily = FontFamily.Monospace,
            )
            if (offer.codAmountCents > 0) {
                Column(horizontalAlignment = Alignment.End) {
                    Text(
                        text = pesos(offer.codAmountCents),
                        color = Tokens.Amber,
                        fontWeight = FontWeight.Bold,
                        fontSize = 14.sp,
                        fontFamily = FontFamily.Monospace,
                    )
                    // Named so nobody reads it as part of the pay.
                    Text("cash to collect", color = Tokens.TextMuted, fontSize = 10.sp)
                }
            }
        }

        Spacer(Modifier.height(8.dp))
        CardBody(offer.card)
        Spacer(Modifier.height(12.dp))

        Button(
            onClick = onAccept,
            // Only one claim in flight at a time: two taps would race for two
            // jobs and field-ops permits exactly one live claim per courier.
            enabled = !anyClaiming,
            colors = ButtonDefaults.buttonColors(
                containerColor = Tokens.Signal,
                contentColor = Tokens.SignalInk,
                disabledContainerColor = Tokens.SurfaceRaised,
                disabledContentColor = Tokens.TextMuted,
            ),
            modifier = Modifier.fillMaxWidth().heightIn(min = Tokens.MinTarget),
        ) {
            if (busy) {
                CircularProgressIndicator(
                    color = Tokens.SignalInk,
                    strokeWidth = 2.dp,
                    modifier = Modifier.size(20.dp),
                )
            } else {
                Text("Accept", fontSize = 16.sp, fontWeight = FontWeight.Bold)
            }
        }
    }
}

/**
 * The card, or an honest gap.
 *
 * A missing or unreadable card is not an error — field-ops forwards whatever the
 * product sent and never reads it, so a newer product could send a shape this
 * build does not know. The pay and the cash are on the assignment itself, so a
 * courier can still judge the job.
 */
@Composable
private fun CardBody(card: OfferCard?) {
    if (card == null || card.isEmpty()) {
        Text(
            "No details for this job yet",
            color = Tokens.TextMuted,
            fontSize = 13.sp,
        )
        return
    }

    Text(
        text = card.headline(),
        color = Tokens.Text,
        fontWeight = FontWeight.Bold,
        fontSize = 15.sp,
    )

    if (card.vendors.isNotEmpty()) {
        Spacer(Modifier.height(4.dp))
        Text(
            text = card.vendors.joinToString(" → "),
            color = Tokens.TextMuted,
            fontSize = 13.sp,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
        )
    }

    if (card.verticals.isNotEmpty() || card.temperature.isNotEmpty()) {
        Spacer(Modifier.height(8.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(5.dp)) {
            card.verticals.forEach { Badge(it, verticalColor(it)) }
            card.temperature.forEach { Badge(it, temperatureColor(it)) }
        }
    }

    if (card.unknownVersion) {
        Spacer(Modifier.height(8.dp))
        Text(
            "Update the app to see everything about this job",
            color = Tokens.Amber,
            fontSize = 11.sp,
        )
    }
}

@Composable
private fun NoticeLine(text: String) {
    Text(
        text = text,
        color = Tokens.Amber,
        fontSize = 13.sp,
        modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 6.dp),
    )
}

@Composable
private fun Centered(title: String, subtitle: String) {
    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
        Column(horizontalAlignment = Alignment.CenterHorizontally) {
            Text(title, color = Tokens.Text, fontWeight = FontWeight.Bold, fontSize = 17.sp)
            Spacer(Modifier.height(4.dp))
            Text(subtitle, color = Tokens.TextMuted, fontSize = 13.sp)
        }
    }
}
