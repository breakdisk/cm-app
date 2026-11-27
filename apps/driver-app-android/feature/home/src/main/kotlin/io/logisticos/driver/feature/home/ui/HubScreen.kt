package io.logisticos.driver.feature.home.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Navigation
import androidx.compose.material.icons.filled.QrCodeScanner
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.hilt.navigation.compose.hiltViewModel
import io.logisticos.driver.core.database.entity.TaskEntity
import io.logisticos.driver.core.database.entity.TaskType
import io.logisticos.driver.core.designsystem.*
import io.logisticos.driver.feature.home.presentation.HubViewModel

/**
 * Hub Operations screen (the design's STACK tab) — lists the driver's open
 * HUB_DROP and RETURN tasks in stop-order. Tapping a task hands off to the
 * existing arrival → pickup confirmation flow (the same screen that already
 * handles HUB_DROP).
 *
 * Rationale: drivers running a hub-drop circuit don't want to scroll past
 * doorstep deliveries on the regular route screen. One-tap entry to the
 * hub work-list keeps the sortation flow tight.
 *
 * @param onBack null where this is a bottom tab (no back affordance).
 * @param onOpenHubScan opens hub mode to scan pieces; hidden when null.
 */
@Composable
fun HubScreen(
    onSelectTask: (taskId: String) -> Unit,
    onBack: (() -> Unit)?,
    onOpenHubScan: (() -> Unit)? = null,
    viewModel: HubViewModel = hiltViewModel(),
) {
    val state by viewModel.uiState.collectAsState()
    val c = LocalMoveColors.current

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(c.ground)
    ) {
        MoveScreenHeader(label = "Stack · hub drops & returns", title = "Hub work", onBack = onBack)

        LazyColumn(
            modifier = Modifier.fillMaxSize(),
            contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 4.dp, bottom = 24.dp),
            verticalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            if (onOpenHubScan != null) {
                item(key = "hub-scan") {
                    MoveActionPanel(
                        kicker = "Hub mode",
                        title = "Scan at the hub",
                        body = "Receive, sort, load or flag pieces against the manifest.",
                        icon = Icons.Filled.QrCodeScanner,
                        onClick = onOpenHubScan,
                    )
                }
            }

            when {
                state.isLoading -> item(key = "loading") {
                    Box(modifier = Modifier.fillMaxWidth().padding(top = 32.dp), contentAlignment = Alignment.Center) {
                        CircularProgressIndicator(color = c.accent)
                    }
                }
                state.hubTasks.isEmpty() -> item(key = "empty") {
                    MoveNotice(
                        title = "No hub drop-offs pending",
                        body = "Returns and hub drops for this shift appear here, in stop order.",
                        tone = MoveTone.Neutral,
                    )
                }
                else -> {
                    item(key = "heading") {
                        MoveLabel(
                            "${state.hubTasks.size} to confirm at the dock",
                            Modifier.padding(top = 8.dp),
                            dot = c.amber,
                        )
                    }
                    items(state.hubTasks, key = { it.id }) { task ->
                        HubTaskCard(task = task, onGo = { onSelectTask(task.id) })
                    }
                }
            }
        }
    }
}

@Composable
private fun HubTaskCard(task: TaskEntity, onGo: () -> Unit) {
    val c = LocalMoveColors.current
    MovePanel {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            MovePill(
                text = if (task.taskType == TaskType.HUB_DROP) "Hub drop" else "Return",
                fg = c.amber,
                bg = c.amberPanel,
            )
            Text("Stop ${task.stopOrder}", color = c.muted, fontSize = 13.sp)
        }
        Text(
            task.awb,
            color = c.ink,
            fontFamily = Condensed,
            fontWeight = FontWeight.Bold,
            fontSize = 26.sp,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.padding(top = 10.dp),
        )
        Text(task.address, color = c.muted, fontSize = 15.sp, modifier = Modifier.padding(top = 4.dp))
        Spacer(Modifier.height(14.dp))
        MoveBigButton(
            label = "GO TO THE HUB",
            onClick = onGo,
            filled = false,
            icon = Icons.Filled.Navigation,
            height = 56.dp,
        )
    }
}
