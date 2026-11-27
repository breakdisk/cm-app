package io.logisticos.driver.feature.delivery.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.hilt.navigation.compose.hiltViewModel
import io.logisticos.driver.core.common.TaskSyncBus
import io.logisticos.driver.core.designsystem.*
import io.logisticos.driver.core.network.service.LeaveQuoteData
import io.logisticos.driver.feature.delivery.presentation.LeaveViewModel
import io.logisticos.driver.feature.delivery.presentation.reasonsFor
import io.logisticos.driver.feature.delivery.presentation.refusalMessage

/**
 * "Before you decide": what leaving this job costs, as the server priced it.
 *
 * A drop (before the grace clock ran out) carries the fee and sends the job
 * back to other drivers. A release (after it) is free and pays for the wait.
 * The screen never decides which — if the clock runs out while the driver
 * reads, the next quote says release and the copy follows.
 */
@Composable
fun LeaveScreen(
    taskId: String,
    /** Last known fix, sent with the leave for the audit record. */
    lastLat: Double? = null,
    lastLng: Double? = null,
    onDone: () -> Unit,
    onBack: () -> Unit,
    viewModel: LeaveViewModel = hiltViewModel(),
) {
    val state by viewModel.uiState.collectAsState()
    val c = LocalMoveColors.current

    LaunchedEffect(taskId) { viewModel.load(taskId) }

    // Done: take the stop off the home list now, and let the next sync prune
    // the job's other legs, which the server cancelled with it.
    LaunchedEffect(state.applied) {
        if (state.applied != null) {
            TaskSyncBus.markLocallyCompleted(taskId)
            TaskSyncBus.requestSync()
            onDone()
        }
    }

    val quote = state.quote
    val release = quote?.mode == "release"
    val tone = if (release) MoveTone.Accent else MoveTone.Penalty

    Column(
        Modifier
            .fillMaxSize()
            .background(c.ground)
            .verticalScroll(rememberScrollState()),
    ) {
        MoveScreenHeader(label = "Penalty", title = "Before you decide", onBack = onBack)

        if (quote == null) {
            Box(Modifier.fillMaxWidth().padding(top = 64.dp), contentAlignment = Alignment.Center) {
                if (state.loading) CircularProgressIndicator(color = c.accent)
                else state.error?.let { MoveNotice(title = "Couldn't load this", body = it, modifier = Modifier.padding(16.dp)) }
            }
            return@Column
        }

        Column(Modifier.padding(horizontal = 16.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
            if (quote.mode == null) {
                MoveNotice(
                    title = "Not from here",
                    body = refusalMessage(quote.refusal) ?: "You can't leave this stop from here.",
                    tone = MoveTone.Amber,
                )
                MoveBigButton(label = "BACK TO THE STOP", onClick = onBack, filled = false)
                return@Column
            }

            Text(
                if (release) "GRACE PERIOD EXCEEDED" else "POST-ACCEPTANCE DROP",
                color = c.tone(tone), fontSize = 12.sp, letterSpacing = 2.4.sp,
            )
            Text(
                when {
                    release -> "Release it — nothing on you."
                    quote.feeCents > 0 -> "This one carries a fee."
                    else -> "No fee on this one."
                },
                color = c.ink, fontFamily = Condensed, fontWeight = FontWeight.Bold, fontSize = 34.sp, lineHeight = 38.sp,
            )
            Text(
                if (release) {
                    "The customer is ${quote.minutesPastGrace} min past the free time. You can leave without penalty."
                } else {
                    "You've already accepted, so a replacement has to be found. The job goes back to other drivers."
                },
                color = c.muted, fontSize = 15.sp, lineHeight = 21.sp,
            )

            CostsPanel(quote, release, tone)

            AcceptancePanel(quote.acceptanceBefore, quote.acceptanceAfter)

            Text("WHY", color = c.muted, fontSize = 12.sp, letterSpacing = 2.4.sp)
            Row(
                Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                reasonsFor(quote.mode).forEach { reason ->
                    MoveChip(
                        label = reason.label,
                        selected = state.reason == reason.code,
                        onClick = { viewModel.pickReason(reason.code) },
                        tone = tone,
                    )
                }
            }
            MoveTextField(
                value = state.note,
                onValueChange = viewModel::setNote,
                label = "Note (optional)",
                singleLine = false,
            )

            state.error?.let { MoveNotice(title = "Not done", body = it, tone = MoveTone.Penalty) }

            MoveBigButton(
                label = if (release) "RELEASE AND LEAVE" else "DROP THIS JOB",
                onClick = { viewModel.confirm(taskId, lastLat, lastLng) },
                tone = tone,
                enabled = state.reason != null,
                loading = state.submitting,
            )
            MoveBigButton(label = if (release) "KEEP WAITING" else "KEEP THE JOB", onClick = onBack, filled = false, height = 60.dp)
        }

        Spacer(Modifier.navigationBarsPadding().height(24.dp))
    }
}

@Composable
private fun CostsPanel(quote: LeaveQuoteData, release: Boolean, tone: MoveTone) {
    val c = LocalMoveColors.current
    MovePanel(tone = tone) {
        Text("WHAT THIS COSTS YOU", color = c.tone(tone), fontSize = 12.sp, letterSpacing = 2.4.sp)
        Spacer(Modifier.height(10.dp))
        if (release) {
            CostRow("Dispatch penalty", "None", "Waived — the delay is on the customer's side", c.accent)
            CostRow("Acceptance rate", "No change", "Releasing after the free time never counts against you", c.accent)
            // Only when a rate is set: the tenant pays it until customers are charged.
            if (quote.waitingFeeCentsPerHour > 0) {
                CostRow(
                    "Waiting pay to you",
                    pesos(quote.waitingFeeCents),
                    "${quote.minutesPastGrace} min past the free time at ${pesos(quote.waitingFeeCentsPerHour)}/h",
                    c.accent,
                )
            }
        } else {
            CostRow(
                "Dispatch penalty",
                if (quote.feeCents > 0) pesos(quote.feeCents) else "None",
                if (quote.feeCents > 0) "${quote.feePct}% of ${pesos(quote.payoutCents)} · taken from your earnings"
                else "No fee is set for dropping a job",
                if (quote.feeCents > 0) c.penalty else c.accent,
            )
            val before = quote.acceptanceBefore
            val after = quote.acceptanceAfter
            if (before != null && after != null && after != before) {
                CostRow("Acceptance rate", "−${before - after}%", "A dropped job counts against the offers you took", c.penalty)
            }
        }
    }
}

@Composable
private fun CostRow(title: String, value: String, detail: String, valueColor: Color) {
    val c = LocalMoveColors.current
    Row(Modifier.fillMaxWidth().padding(vertical = 6.dp), verticalAlignment = Alignment.Top) {
        Column(Modifier.weight(1f)) {
            Text(title, color = c.ink, fontSize = 15.sp, fontWeight = FontWeight.Bold)
            Text(detail, color = c.muted, fontSize = 13.sp, lineHeight = 18.sp)
        }
        Spacer(Modifier.width(12.dp))
        Text(value, color = valueColor, fontFamily = Condensed, fontWeight = FontWeight.Bold, fontSize = 22.sp)
    }
}

/** Before → after, over the offers this driver saw. Hidden before any. */
@Composable
private fun AcceptancePanel(before: Int?, after: Int?) {
    if (before == null) return
    val c = LocalMoveColors.current
    val shown = after ?: before
    val moved = after != null && after != before
    MovePanel {
        Text("ACCEPTANCE RATE", color = c.muted, fontSize = 12.sp, letterSpacing = 2.4.sp)
        Row(verticalAlignment = Alignment.Bottom) {
            Text("$before%", color = c.ink, fontFamily = Condensed, fontWeight = FontWeight.Bold, fontSize = 36.sp)
            if (moved) {
                Text("  → $after%", color = c.penalty, fontSize = 16.sp, modifier = Modifier.padding(bottom = 6.dp))
            }
        }
        Box(
            Modifier.fillMaxWidth().padding(top = 10.dp).height(6.dp).clip(RoundedCornerShape(3.dp)).background(c.hairline),
        ) {
            Box(
                Modifier
                    .fillMaxWidth(shown.coerceIn(0, 100) / 100f)
                    .fillMaxHeight()
                    .clip(RoundedCornerShape(3.dp))
                    .background(if (moved) c.penalty else c.accent),
            )
        }
        Text(
            if (moved) "Counted over the offers you've seen." else "Nothing here changes it.",
            color = c.muted, fontSize = 13.sp, modifier = Modifier.padding(top = 8.dp),
        )
    }
}
