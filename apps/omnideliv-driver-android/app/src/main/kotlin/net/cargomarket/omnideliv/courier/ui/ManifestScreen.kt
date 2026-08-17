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
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import net.cargomarket.omnideliv.courier.domain.Dropoff
import net.cargomarket.omnideliv.courier.domain.GeofenceAdvice
import net.cargomarket.omnideliv.courier.domain.Leg
import net.cargomarket.omnideliv.courier.domain.Line
import net.cargomarket.omnideliv.courier.domain.Manifest
import net.cargomarket.omnideliv.courier.domain.RailEntry
import net.cargomarket.omnideliv.courier.domain.Stop
import net.cargomarket.omnideliv.courier.domain.currentLeg
import net.cargomarket.omnideliv.courier.domain.rail

/**
 * The screen the app lives on.
 *
 * Focus plus rail. A compact strip of every stop sits on top; one job fills the
 * rest. The reason is §2 of the specification: the Logistics agent can rewrite
 * the route while the courier is mid-tap, and in this layout that change
 * animates *in the rail* while the focus card and its primary action never
 * move. A card stack would shuffle the deck under their thumb; a scrolling list
 * would move the button under the scroll.
 */
@Composable
fun ManifestScreen(
    manifest: Manifest? = null,
    advice: GeofenceAdvice = GeofenceAdvice.NoFix,
    servedFromCache: Boolean = false,
    pendingCount: Int = 0,
    onAdvance: () -> Unit = {},
    onIssue: () -> Unit = {},
) {
    if (manifest == null) {
        Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
            Text("No job right now", color = Tokens.TextMuted, fontSize = 16.sp)
        }
        return
    }

    Column(Modifier.fillMaxSize()) {
        HeaderBar(manifest, servedFromCache, pendingCount)
        Rail(manifest.rail())
        Column(
            Modifier
                .weight(1f)
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 12.dp),
        ) {
            when (val leg = manifest.currentLeg()) {
                is Leg.ToPickup -> PickupCard(leg.stop)
                is Leg.ToDropoff -> DropoffCard(leg.dropoff, manifest.codAmountCents)
                Leg.Done -> Text(
                    "Job complete",
                    color = Tokens.Signal,
                    fontWeight = FontWeight.Bold,
                    fontSize = 20.sp,
                    modifier = Modifier.padding(top = 24.dp),
                )
            }
        }
        // Pinned in the bottom third and never moved by a re-sequence.
        AdvanceControl(manifest, advice, onAdvance, onIssue)
    }
}

@Composable
private fun HeaderBar(manifest: Manifest, servedFromCache: Boolean, pendingCount: Int) {
    Column(
        Modifier
            .fillMaxWidth()
            .background(Tokens.Base)
            .padding(horizontal = 12.dp, vertical = 10.dp),
    ) {
        Row(
            Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                text = "Trip · ${pesos(manifest.tripCents + manifest.tipCents)}",
                color = Tokens.Text,
                fontWeight = FontWeight.Bold,
                fontSize = 15.sp,
            )
            if (manifest.codAmountCents > 0) {
                Text(
                    text = "${pesos(manifest.codAmountCents)} cash",
                    color = Tokens.Amber,
                    fontWeight = FontWeight.Bold,
                    fontSize = 13.sp,
                    fontFamily = FontFamily.Monospace,
                )
            }
        }

        // The render-cache indicator. Says plainly that this is local data
        // rather than letting a stale screen pass as live.
        if (servedFromCache) {
            Spacer(Modifier.height(6.dp))
            Text(
                text = "Offline · showing your last synced route",
                color = Tokens.Amber,
                fontSize = 12.sp,
            )
        }
        // Pending is a first-class state, not a toast. A static count, never a
        // spinner: nothing is in progress with no network, and a spinner that
        // never resolves reads as a hung app.
        if (pendingCount > 0) {
            Spacer(Modifier.height(4.dp))
            Text(
                text = "$pendingCount update${if (pendingCount == 1) "" else "s"} waiting to sync",
                color = Tokens.TextMuted,
                fontSize = 12.sp,
            )
        }
    }
}

@Composable
private fun Rail(entries: List<RailEntry>) {
    Row(
        Modifier
            .fillMaxWidth()
            .background(Tokens.Base)
            .padding(horizontal = 12.dp)
            .padding(bottom = 12.dp),
        horizontalArrangement = Arrangement.spacedBy(5.dp),
    ) {
        val firstPending = entries.indexOfFirst { !it.done }
        entries.forEachIndexed { index, entry ->
            val isCurrent = index == firstPending
            Column(
                Modifier
                    .weight(1f)
                    .clip(RoundedCornerShape(8.dp))
                    .background(if (isCurrent) Tokens.SurfaceRaised else Tokens.Surface)
                    .border(
                        width = 1.dp,
                        color = if (isCurrent) Tokens.Signal else Tokens.Border,
                        shape = RoundedCornerShape(8.dp),
                    )
                    .padding(vertical = 7.dp, horizontal = 6.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                SeqChip(entry.seq, done = entry.done, current = isCurrent)
                Spacer(Modifier.height(5.dp))
                Text(
                    text = entry.label,
                    color = if (isCurrent) Tokens.Signal else Tokens.Text,
                    fontSize = 11.sp,
                    fontWeight = FontWeight.Bold,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    text = when {
                        entry.done -> "done"
                        isCurrent -> "now"
                        entry.isDropoff -> "drop"
                        else -> "next"
                    },
                    color = Tokens.TextMuted,
                    fontSize = 9.sp,
                )
            }
        }
    }
}

@Composable
private fun SeqChip(seq: Int, done: Boolean, current: Boolean) {
    val bg = when {
        done -> Tokens.SignalInk
        current -> Tokens.Signal
        else -> Tokens.SurfaceRaised
    }
    val fg = when {
        done -> Tokens.Signal
        current -> Tokens.SignalInk
        else -> Tokens.TextMuted
    }
    Box(
        Modifier
            .size(26.dp)
            .clip(RoundedCornerShape(8.dp))
            .background(bg),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            text = if (done) "✓" else seq.toString(),
            color = fg,
            fontSize = 12.sp,
            fontWeight = FontWeight.Bold,
        )
    }
}

@Composable
private fun PickupCard(stop: Stop) {
    Column(Modifier.fillMaxWidth().padding(top = 4.dp)) {
        Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            Badge("▲ Pick up", Tokens.Cyan)
            // The vertical is why the manifest comes from omnideliv rather than
            // field-ops: the platform tier cannot know a stop is a pharmacy.
            Badge(stop.vertical.uppercase(), verticalColor(stop.vertical))
        }
        Spacer(Modifier.height(8.dp))
        Text(stop.vendorName, color = Tokens.Text, fontSize = 20.sp, fontWeight = FontWeight.Bold)
        Spacer(Modifier.height(4.dp))
        Text(stop.address, color = Tokens.TextMuted, fontSize = 13.sp)

        Spacer(Modifier.height(12.dp))
        Column(
            Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(10.dp))
                .background(Tokens.Surface)
                .padding(11.dp),
        ) {
            Text("COLLECT", color = Tokens.TextMuted, fontSize = 10.sp, fontWeight = FontWeight.Bold)
            Spacer(Modifier.height(6.dp))
            stop.lines.forEach { LineRow(it) }
        }
    }
}

@Composable
private fun LineRow(line: Line) {
    Column(Modifier.padding(bottom = 6.dp)) {
        Text(
            text = "${line.qty} × ${line.itemName}",
            color = Tokens.Text,
            fontSize = 14.sp,
        )
        line.modifiers.forEach {
            Text(it, color = Tokens.TextMuted, fontSize = 12.sp)
        }
    }
}

@Composable
private fun DropoffCard(dropoff: Dropoff, codCents: Long) {
    Column(Modifier.fillMaxWidth().padding(top = 4.dp)) {
        Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            Badge("▼ Deliver", Tokens.Signal)
            if (codCents > 0) Badge("${pesos(codCents)} CASH", Tokens.Amber)
        }
        Spacer(Modifier.height(8.dp))
        Text(
            text = dropoff.customerName ?: "Customer",
            color = Tokens.Text,
            fontSize = 20.sp,
            fontWeight = FontWeight.Bold,
        )
        // No street address exists on an order — checkout captures coordinates
        // only. Showing the pin honestly beats inventing a line of address.
        Spacer(Modifier.height(4.dp))
        Text(
            text = "%.5f, %.5f".format(dropoff.lat, dropoff.lng),
            color = Tokens.TextMuted,
            fontSize = 13.sp,
            fontFamily = FontFamily.Monospace,
        )
        dropoff.customerPhone?.let {
            Spacer(Modifier.height(8.dp))
            Text("Call $it", color = Tokens.Cyan, fontSize = 14.sp, fontWeight = FontWeight.Bold)
        }
        dropoff.notes?.let {
            Spacer(Modifier.height(8.dp))
            Text(it, color = Tokens.Text, fontSize = 13.sp)
        }
    }
}

@Composable
private fun Badge(text: String, color: androidx.compose.ui.graphics.Color) {
    Box(
        Modifier
            .clip(RoundedCornerShape(5.dp))
            .background(Tokens.Surface)
            .border(1.dp, color, RoundedCornerShape(5.dp))
            .padding(horizontal = 7.dp, vertical = 3.dp),
    ) {
        Text(text, color = color, fontSize = 9.sp, fontWeight = FontWeight.Bold)
    }
}

/**
 * The graduated control.
 *
 * Tap for the reversible steps, and the geofence renders as a distance chip —
 * never a disabled button. A hard gate strands a courier standing at the door
 * in a lift lobby, and with cash on delivery the money is already in their hand
 * when the door closes, so refusing the tap cannot un-collect it.
 */
@Composable
private fun AdvanceControl(
    manifest: Manifest,
    advice: GeofenceAdvice,
    onAdvance: () -> Unit,
    onIssue: () -> Unit,
) {
    val label = when (manifest.currentLeg()) {
        is Leg.ToPickup -> "Picked up"
        is Leg.ToDropoff -> "Delivered"
        Leg.Done -> "Done"
    }

    Column(
        Modifier
            .fillMaxWidth()
            .background(Tokens.Base)
            .padding(12.dp),
    ) {
        GeofenceChip(advice)
        Spacer(Modifier.height(8.dp))
        Button(
            onClick = onAdvance,
            enabled = manifest.currentLeg() != Leg.Done,
            modifier = Modifier
                .fillMaxWidth()
                .heightIn(min = Tokens.MinTarget),
            shape = RoundedCornerShape(12.dp),
            colors = ButtonDefaults.buttonColors(
                containerColor = Tokens.Signal,
                contentColor = Tokens.SignalInk,
            ),
        ) {
            Text(label, fontSize = 16.sp, fontWeight = FontWeight.Bold)
        }
        Spacer(Modifier.height(7.dp))
        Button(
            onClick = onIssue,
            modifier = Modifier
                .fillMaxWidth()
                .heightIn(min = 44.dp),
            shape = RoundedCornerShape(12.dp),
            colors = ButtonDefaults.buttonColors(
                containerColor = Tokens.SurfaceRaised,
                contentColor = Tokens.Text,
            ),
        ) {
            Text("Report an issue", fontSize = 13.sp)
        }
    }
}

@Composable
private fun GeofenceChip(advice: GeofenceAdvice) {
    val (text, color) = when (advice) {
        is GeofenceAdvice.AtStop -> "At the address · ${advice.meters} m" to Tokens.Signal
        is GeofenceAdvice.Away -> "GPS says ${advice.meters} m away · flagged, not blocked" to Tokens.Amber
        GeofenceAdvice.NoFix -> "No GPS fix · you can still continue" to Tokens.TextMuted
    }
    Text(text, color = color, fontSize = 12.sp)
}

/** Cents to a peso string. Integer maths only — no float touches money. */
private fun pesos(cents: Long): String = "₱${cents / 100}.${(cents % 100).toString().padStart(2, '0')}"

private fun verticalColor(vertical: String) = when (vertical.lowercase()) {
    "pharmacy" -> Tokens.Plasma
    "florist" -> Tokens.Plasma
    "grocery" -> Tokens.Cyan
    else -> Tokens.Amber
}
