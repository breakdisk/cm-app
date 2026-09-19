package io.logisticos.driver.feature.profile.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.CalendarMonth
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.hilt.navigation.compose.hiltViewModel
import io.logisticos.driver.core.designsystem.*
import io.logisticos.driver.core.network.service.LeadMoveItem
import io.logisticos.driver.feature.profile.presentation.HomeMovesViewModel
import io.logisticos.driver.feature.profile.presentation.crewLine
import io.logisticos.driver.feature.profile.presentation.dayLabel
import io.logisticos.driver.feature.profile.presentation.whenLabel

/**
 * The lead's reserved whole-home moves. Each one is theirs from the claim; the
 * day's job appears on the board twelve hours before the move. Surveys are
 * run from here.
 */
@Composable
fun HomeMovesScreen(
    onBack: () -> Unit,
    onOpenSurvey: (shipmentId: String) -> Unit,
    onOpenAvailability: () -> Unit,
    viewModel: HomeMovesViewModel = hiltViewModel(),
) {
    val state by viewModel.uiState.collectAsState()
    val c = LocalMoveColors.current
    // Reloaded on every return, so a survey just sent is reflected.
    LaunchedEffect(Unit) { viewModel.load() }

    Column(
        Modifier
            .fillMaxSize()
            .background(c.ground)
            .verticalScroll(rememberScrollState()),
    ) {
        MoveScreenHeader(
            label = "Whole-home moves",
            title = "Your reserved moves",
            onBack = onBack,
            actions = {
                MoveSquareButton(Icons.Filled.CalendarMonth, "Working days", onOpenAvailability)
                if (!state.loading) MoveSquareButton(Icons.Filled.Refresh, "Refresh", viewModel::load)
            },
        )
        Column(Modifier.padding(horizontal = 16.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
            when {
                state.loading && state.moves.isEmpty() -> Box(Modifier.fillMaxWidth().padding(top = 48.dp), contentAlignment = Alignment.Center) {
                    CircularProgressIndicator(color = c.accent, strokeWidth = 2.dp)
                }
                state.error != null && state.moves.isEmpty() -> {
                    MoveNotice("Couldn't load your moves", state.error!!, tone = MoveTone.Penalty)
                    MoveBigButton("TRY AGAIN", onClick = viewModel::load, filled = false, height = 56.dp)
                }
                state.moves.isEmpty() -> MoveNotice(
                    title = "No moves reserved",
                    body = "Whole-home moves you claim from the board land here, with their survey. Keep your working days current so you're offered the days you work.",
                    tone = MoveTone.Neutral,
                )
                else -> state.moves.forEach { m -> MoveCard(m, onSurvey = { onOpenSurvey(m.shipmentId) }) }
            }
        }
        Spacer(Modifier.navigationBarsPadding().height(24.dp))
    }
}

@Composable
private fun MoveCard(m: LeadMoveItem, onSurvey: () -> Unit) {
    val c = LocalMoveColors.current
    MovePanel(tone = if (m.activated) MoveTone.Accent else MoveTone.Neutral) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(
                dayLabel(m.moveDate),
                color = c.ink, fontFamily = Condensed, fontWeight = FontWeight.Bold, fontSize = 24.sp,
                modifier = Modifier.weight(1f),
            )
            if (m.activated) MoveStatePill("ON THE BOARD", c.accent)
            if (m.largeEstate) MoveStatePill("LARGE ESTATE", c.amber)
            if (m.international) MoveStatePill("INTERNATIONAL", c.amber)
        }
        Text(m.customerName.ifBlank { "Customer" }, color = c.ink, fontSize = 16.sp, fontWeight = FontWeight.SemiBold, modifier = Modifier.padding(top = 4.dp))
        m.trackingNumber?.let { Text(it, color = c.muted, fontSize = 12.sp, fontFamily = FontFamily.Monospace) }
        Spacer(Modifier.height(12.dp))
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            MoveDetailRow("From", m.pickup)
            MoveDetailRow("To", m.dropoff)
            MoveDetailRow("Your crew", crewLine(m.trucks, m.crewTotal))
            m.leadPayoutCents?.takeIf { it > 0 }?.let { MoveDetailRow("Your pay", "${pesos(it)} after commission", c.success) }
            MoveDetailRow("Arrive", whenLabel(m.moveAt))
            MoveDetailRow("Survey", m.surveyAt?.let(::whenLabel) ?: "Not needed for this size")
        }
        Spacer(Modifier.height(14.dp))
        MoveBigButton(
            label = if (m.surveyAt != null) "SURVEY THE HOME" else "ADD ITEMS FOUND",
            onClick = onSurvey,
            filled = m.surveyAt != null && !m.activated,
            height = 56.dp,
        )
    }
}
