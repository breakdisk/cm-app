package io.logisticos.driver.feature.profile.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.hilt.navigation.compose.hiltViewModel
import io.logisticos.driver.core.designsystem.Condensed
import io.logisticos.driver.core.designsystem.LocalMoveColors
import io.logisticos.driver.core.designsystem.MovePanel
import io.logisticos.driver.core.designsystem.MoveTone
import io.logisticos.driver.core.designsystem.hoursMinutes
import io.logisticos.driver.core.network.service.HosData
import io.logisticos.driver.feature.profile.presentation.HosViewModel
import kotlinx.coroutines.delay

/**
 * Hours of service (driver design, compliance). Renders nothing until the clock
 * loads, or when driver-ops has none to give — never a made-up figure.
 */
@Composable
fun HosPanel(modifier: Modifier = Modifier, viewModel: HosViewModel = hiltViewModel()) {
    val state by viewModel.uiState.collectAsState()
    LaunchedEffect(Unit) { viewModel.refresh() }
    val clock = state.clock ?: return
    HosClockPanel(clock = clock, fetchedAtMillis = state.fetchedAtMillis, modifier = modifier)
}

@Composable
fun HosClockPanel(clock: HosData, fetchedAtMillis: Long, modifier: Modifier = Modifier) {
    val c = LocalMoveColors.current
    val r = rememberHosReading(clock, fetchedAtMillis)
    val fill = when {
        r.over -> c.penalty
        r.remaining < 60 -> c.amber
        else -> c.accent
    }
    MovePanel(modifier, tone = if (r.over) MoveTone.Penalty else MoveTone.Neutral) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text("HOURS OF SERVICE", color = c.accent, fontSize = 12.sp, letterSpacing = 2.4.sp)
            Text("Last ${clock.windowHours} h", color = c.muted, fontSize = 13.sp)
        }
        Row(
            modifier = Modifier.fillMaxWidth().padding(top = 16.dp),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.Bottom,
        ) {
            Text(if (r.onDuty) "On duty now" else "On duty", color = c.ink, fontSize = 15.sp)
            Text(
                hoursMinutes(r.used),
                color = if (r.over) c.penalty else c.ink,
                fontFamily = Condensed,
                fontWeight = FontWeight.Bold,
                fontSize = 30.sp,
            )
        }
        // One cell per hour of the limit, as the design draws it; a cell lights
        // as soon as that hour is started.
        val cells = (r.limit / 60).toInt().coerceIn(1, 24)
        Row(
            modifier = Modifier.fillMaxWidth().padding(top = 9.dp),
            horizontalArrangement = Arrangement.spacedBy(3.dp),
        ) {
            repeat(cells) { i ->
                Box(
                    Modifier
                        .weight(1f)
                        .height(8.dp)
                        .clip(RoundedCornerShape(2.dp))
                        .background(if (r.used > r.limit * i / cells) fill else c.chip)
                )
            }
        }
        Text(
            when {
                r.over -> "${hoursMinutes(r.used - r.limit)} over the ${hoursMinutes(r.limit)} limit. Recorded for your logs — no loads are held back on it."
                r.onDuty -> "${hoursMinutes(r.remaining)} left of ${hoursMinutes(r.limit)} · this stretch ${hoursMinutes(r.stretch)}"
                else -> "${hoursMinutes(r.remaining)} left of ${hoursMinutes(r.limit)} · off duty"
            },
            color = if (r.over) c.penalty else c.muted,
            fontSize = 13.sp,
            modifier = Modifier.padding(top = 7.dp),
        )
    }
}

/** One line for the load board's duty card. */
@Composable
fun HosSummaryLine(clock: HosData, fetchedAtMillis: Long, modifier: Modifier = Modifier) {
    val c = LocalMoveColors.current
    val r = rememberHosReading(clock, fetchedAtMillis)
    Text(
        if (r.over) "Hours of service: ${hoursMinutes(r.used)} on duty — over the ${hoursMinutes(r.limit)} limit"
        else "Hours of service: ${hoursMinutes(r.used)} of ${hoursMinutes(r.limit)} · ${hoursMinutes(r.remaining)} left",
        color = when {
            r.over -> c.penalty
            r.remaining < 60 -> c.amber
            else -> c.muted
        },
        fontSize = 13.sp,
        modifier = modifier,
    )
}

private data class HosReading(
    val used: Long,
    val remaining: Long,
    val limit: Long,
    val over: Boolean,
    val onDuty: Boolean,
    val stretch: Long,
)

/**
 * The server's figures plus the minutes since they were fetched, ticking every
 * 30 s while the driver is on duty. Off duty nothing changes until the next fetch.
 */
@Composable
private fun rememberHosReading(clock: HosData, fetchedAtMillis: Long): HosReading {
    val onDuty = clock.onDutySince != null
    val now by produceState(System.currentTimeMillis(), onDuty, fetchedAtMillis) {
        value = System.currentTimeMillis()
        while (onDuty) {
            delay(30_000)
            value = System.currentTimeMillis()
        }
    }
    val elapsed = if (onDuty) ((now - fetchedAtMillis) / 60_000).coerceAtLeast(0) else 0L
    val used = clock.onDutyMinutes + elapsed
    val limit = clock.limitMinutes
    return HosReading(
        used = used,
        remaining = (limit - used).coerceAtLeast(0),
        limit = limit,
        over = used > limit,
        onDuty = onDuty,
        stretch = clock.currentStretchMinutes + elapsed,
    )
}
