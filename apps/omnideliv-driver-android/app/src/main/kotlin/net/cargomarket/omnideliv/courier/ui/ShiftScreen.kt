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
import androidx.compose.material3.TextButton
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
import android.Manifest
import android.content.pm.PackageManager
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.ui.platform.LocalContext
import androidx.core.content.ContextCompat
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.hilt.navigation.compose.hiltViewModel
import net.cargomarket.omnideliv.courier.data.location.ShiftLocationService
import net.cargomarket.omnideliv.courier.domain.OfferCard
import net.cargomarket.omnideliv.courier.domain.documentsBadge
import net.cargomarket.omnideliv.courier.domain.documentsBadgeDescription

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
    /**
     * Read only for the count on the Documents link. [ShiftViewModel] is left
     * alone deliberately — it is a polling offer machine, and this view model
     * already computes exactly this number for the compliance screen itself.
     */
    complianceVm: ComplianceViewModel = hiltViewModel(),
    onClaimed: (orderId: String, assignmentId: String) -> Unit = { _, _ -> },
    onEarnings: () -> Unit = {},
    onCompliance: () -> Unit = {},
) {
    val state by vm.state.collectAsState()
    val compliance by complianceVm.state.collectAsState()
    val context = LocalContext.current

    // Once per entry, not on the offer poll: document status changes on a human
    // reviewer's timescale, not a six-second one.
    //
    // A failure here is silent by design. The load already degrades to
    // `failed = true`, the badge simply does not appear, and a compliance outage
    // must not take the screen a courier works from with it.
    //
    // Worth having as a side effect: `GET /me/profile` opens the profile
    // lazily, so every courier who reaches this screen gets one. That is the
    // backfill happening on its own.
    LaunchedEffect(Unit) { complianceVm.load() }

    // Going on duty is what starts location streaming, and it must not start
    // without permission — a foreground location service that cannot read a
    // location is a notification telling the courier something untrue.
    val askLocation = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestPermission(),
    ) { allowed ->
        if (allowed) vm.goOnline()
    }

    fun setDuty(on: Boolean) {
        if (!on) {
            vm.goOffline()
            return
        }
        val granted = ContextCompat.checkSelfPermission(
            context, Manifest.permission.ACCESS_FINE_LOCATION,
        ) == PackageManager.PERMISSION_GRANTED

        if (granted) {
            vm.goOnline()
        } else {
            askLocation.launch(Manifest.permission.ACCESS_FINE_LOCATION)
        }
    }

    // Location streaming follows the *state*, not the tap.
    //
    // Going on duty is now a request the server can refuse, and starting the
    // service on the tap would leave it reporting a courier's location while
    // they are not on shift at all. Keyed on the boolean rather than on `state`
    // so a routine offer-list refresh does not restart the service every six
    // seconds.
    //
    // `!is Offline` rather than `is Online`, because a courier who has claimed
    // a job is working and must still be tracked — that is the whole point of
    // the manifest screen having a live position behind it.
    val onShift = state !is ShiftState.Offline
    LaunchedEffect(onShift) {
        if (onShift) ShiftLocationService.start(context) else ShiftLocationService.stop(context)
    }

    // Navigation is a side effect of state, not of the tap. A tap that claimed
    // successfully but was followed by a config change would otherwise lose the
    // navigation and strand the courier on an empty offer list holding a job.
    LaunchedEffect(state) {
        (state as? ShiftState.Claimed)?.let { onClaimed(it.externalRef, it.assignmentId) }
    }

    Column(Modifier.fillMaxSize().background(Tokens.Base)) {
        DutyBar(
            online = state !is ShiftState.Offline,
            stale = (state as? ShiftState.Online)?.stale == true,
            onToggle = { on -> setDuty(on) },
            onEarnings = onEarnings,
            onCompliance = onCompliance,
            outstandingDocuments = compliance.outstanding,
        )

        when (val s = state) {
            is ShiftState.Offline -> Centered(
                "You are off duty",
                s.notice ?: "Go on duty to start receiving offers.",
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
private fun DutyBar(
    online: Boolean,
    stale: Boolean,
    onToggle: (Boolean) -> Unit,
    onEarnings: () -> Unit,
    onCompliance: () -> Unit,
    /** How many documents are the courier's move. Zero renders no badge. */
    outstandingDocuments: Int = 0,
) {
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

        // Their own row rather than squeezed beside the duty switch. Two text
        // links and a switch on one line leaves each of them under the 48 dp
        // touch target on a small phone, and this is a screen used one-handed
        // on a motorbike seat.
        Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
            TextButton(onClick = onEarnings) {
                Text("Earnings", color = Tokens.Cyan, fontSize = 13.sp)
            }
            TextButton(
                onClick = onCompliance,
                // The whole control speaks, rather than the badge reading out a
                // bare number next to a word.
                modifier = Modifier.semantics {
                    contentDescription = documentsBadgeDescription(outstandingDocuments)
                },
            ) {
                Text("Documents", color = Tokens.Cyan, fontSize = 13.sp)
                documentsBadge(outstandingDocuments)?.let { count ->
                    Spacer(Modifier.size(6.dp))
                    Text(
                        text = count,
                        color = Tokens.SignalInk,
                        fontSize = 11.sp,
                        fontWeight = FontWeight.Bold,
                        modifier = Modifier
                            .clip(RoundedCornerShape(9.dp))
                            .background(Tokens.Amber)
                            .padding(horizontal = 6.dp, vertical = 1.dp),
                    )
                }
            }
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
