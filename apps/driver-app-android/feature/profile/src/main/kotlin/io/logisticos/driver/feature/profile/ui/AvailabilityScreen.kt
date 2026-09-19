package io.logisticos.driver.feature.profile.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.hilt.navigation.compose.hiltViewModel
import io.logisticos.driver.core.designsystem.*
import io.logisticos.driver.core.network.service.ProviderProfileDto
import io.logisticos.driver.feature.profile.presentation.AvailabilityViewModel
import io.logisticos.driver.feature.profile.presentation.OFF_DAY_HORIZON_DAYS
import io.logisticos.driver.feature.profile.presentation.WEEKDAYS
import io.logisticos.driver.feature.profile.presentation.dayLabel
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneOffset

/**
 * The lead's working days and days off. Customers are offered a home-move
 * window only while enough onboarded leads work it, so this is the calendar
 * their dates come from. Operations' onboarding is shown, not edited.
 */
@Composable
fun AvailabilityScreen(
    onBack: () -> Unit,
    viewModel: AvailabilityViewModel = hiltViewModel(),
) {
    val s by viewModel.uiState.collectAsState()
    val c = LocalMoveColors.current
    var picking by remember { mutableStateOf(false) }

    Column(
        Modifier
            .fillMaxSize()
            .background(c.ground)
            .verticalScroll(rememberScrollState()),
    ) {
        MoveScreenHeader(label = "Availability", title = "Your working days", onBack = onBack)
        Column(Modifier.padding(horizontal = 16.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
            when {
                s.loading && s.profile == null -> Box(Modifier.fillMaxWidth().padding(top = 48.dp), contentAlignment = Alignment.Center) {
                    CircularProgressIndicator(color = c.accent, strokeWidth = 2.dp)
                }
                s.profile == null -> {
                    MoveNotice("Couldn't load your availability", s.error ?: "Try again.", tone = MoveTone.Penalty)
                    MoveBigButton("TRY AGAIN", onClick = viewModel::load, filled = false, height = 56.dp)
                }
                else -> {
                    Onboarding(s.profile!!)

                    MoveLabel("Days you work")
                    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                        WEEKDAYS.forEachIndexed { i, name ->
                            DayToggle(name, on = i in s.workingDays, onToggle = { viewModel.toggle(i) }, modifier = Modifier.weight(1f))
                        }
                    }
                    if (s.workingDays.isEmpty()) {
                        MoveNotice("No working days", "You won't be offered home moves until you pick at least one.", tone = MoveTone.Amber)
                    }

                    MoveLabel("Days off")
                    if (s.offDays.isEmpty()) {
                        Text("None coming up.", color = c.muted, fontSize = 14.sp)
                    }
                    s.offDays.forEach { d ->
                        MovePanel(padding = 12.dp) {
                            Row(verticalAlignment = Alignment.CenterVertically) {
                                Text(dayLabel(d), color = c.ink, fontSize = 16.sp, fontWeight = FontWeight.SemiBold, modifier = Modifier.weight(1f))
                                MoveSquareButton(Icons.Filled.Close, "Remove ${dayLabel(d)}", { viewModel.removeOff(d) })
                            }
                        }
                    }
                    MoveBigButton("ADD A DAY OFF", onClick = { picking = true }, filled = false, height = 56.dp)
                    Text(
                        "A day off doesn't drop moves you've already reserved — tell operations if you can't make one.",
                        color = c.muted, fontSize = 13.sp,
                    )

                    s.notice?.let { MoveNotice("Not saved", it, tone = MoveTone.Amber) }
                    if (s.saved && !s.dirty) {
                        Text("Saved. New dates open or close for customers straight away.", color = c.accent, fontSize = 13.sp)
                    }
                    MoveBigButton("SAVE", onClick = viewModel::save, enabled = s.dirty, loading = s.saving)
                }
            }
        }
        Spacer(Modifier.navigationBarsPadding().height(24.dp))
    }

    if (picking) {
        OffDayPicker(
            onPick = { picking = false; viewModel.addOff(it) },
            onDismiss = { picking = false },
        )
    }
}

@Composable
private fun Onboarding(p: ProviderProfileDto) {
    val lines = buildList {
        if ("freight_move" in p.serviceLines) add("Freight moves")
        if ("home_move" in p.serviceLines) add("Whole-home moves")
    }
    val coverage = p.coverage.map { if (it == "international") "International" else "Local" }
    MovePanel {
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            MoveDetailRow("Offered", lines.joinToString().ifBlank { "—" })
            MoveDetailRow("Coverage", coverage.joinToString().ifBlank { "—" })
            if ("home_move" in p.serviceLines) {
                MoveDetailRow("Your team", "${p.fleetTrucks} truck${if (p.fleetTrucks == 1) "" else "s"} · ${p.registeredHelpers} helpers")
                if (p.multiTruckCapable) MoveDetailRow("Lead", "Multi-truck — offered Large Estates")
                MoveDetailRow("Jobs a day", "${p.maxDailyJobs}")
            }
        }
        Text("Set by operations at onboarding.", color = LocalMoveColors.current.muted, fontSize = 12.sp, modifier = Modifier.padding(top = 10.dp))
    }
}

@Composable
private fun DayToggle(name: String, on: Boolean, onToggle: () -> Unit, modifier: Modifier = Modifier) {
    val c = LocalMoveColors.current
    val shape = RoundedCornerShape(14.dp)
    Box(
        modifier
            .height(56.dp)
            .clip(shape)
            .background(if (on) c.accentPanel else c.chip)
            .border(1.dp, if (on) c.accentBorder else c.hairline, shape)
            .toggleable(value = on, role = Role.Checkbox, onValueChange = { onToggle() }),
        contentAlignment = Alignment.Center,
    ) {
        Text(name, color = if (on) c.accent else c.muted, fontSize = 13.sp, fontWeight = if (on) FontWeight.Bold else FontWeight.Medium, maxLines = 1)
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun OffDayPicker(onPick: (LocalDate) -> Unit, onDismiss: () -> Unit) {
    val today = remember { LocalDate.now() }
    val last = remember { today.plusDays(OFF_DAY_HORIZON_DAYS) }
    val state = rememberDatePickerState(
        selectableDates = object : SelectableDates {
            override fun isSelectableDate(utcTimeMillis: Long): Boolean {
                val d = Instant.ofEpochMilli(utcTimeMillis).atZone(ZoneOffset.UTC).toLocalDate()
                return !d.isBefore(today) && !d.isAfter(last)
            }

            override fun isSelectableYear(year: Int): Boolean = year in today.year..last.year
        },
    )
    DatePickerDialog(
        onDismissRequest = onDismiss,
        confirmButton = {
            TextButton(
                enabled = state.selectedDateMillis != null,
                onClick = {
                    state.selectedDateMillis?.let { onPick(Instant.ofEpochMilli(it).atZone(ZoneOffset.UTC).toLocalDate()) }
                },
            ) { Text("Take it off") }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    ) {
        DatePicker(state = state)
    }
}
