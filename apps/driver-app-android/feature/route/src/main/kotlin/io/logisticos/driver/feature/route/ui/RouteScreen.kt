package io.logisticos.driver.feature.route.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.draggable
import androidx.compose.foundation.gestures.rememberDraggableState
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.DragHandle
import androidx.compose.material.icons.filled.Inventory2
import androidx.compose.material.icons.filled.Navigation
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.hilt.navigation.compose.hiltViewModel
import io.logisticos.driver.core.database.entity.TaskEntity
import io.logisticos.driver.core.database.entity.TaskStatus
import io.logisticos.driver.core.designsystem.*
import io.logisticos.driver.feature.route.presentation.RouteViewModel
import kotlin.math.roundToInt

/** Space between stop cards; part of one reorder step when dragging. */
private val CardGap = 14.dp

/**
 * The circuit (driver design): the shift's stops in order, the next one lit,
 * each with a glove-sized NAVIGATE. Drag a stop's handle to reorder.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun RouteScreen(
    shiftId: String,
    onNavigateToStop: (taskId: String) -> Unit,
    /** Opens hub mode to clear the manifest before rolling; hidden when null. */
    onScanManifest: (() -> Unit)? = null,
) {
    val viewModel: RouteViewModel = hiltViewModel(
        creationCallback = { factory: RouteViewModel.Factory -> factory.create(shiftId) }
    )
    val state by viewModel.uiState.collectAsState()
    val c = LocalMoveColors.current

    // Selected completed task for the detail bottom-sheet. Read-only —
    // we don't yet store completed_at / pod_id on TaskEntity, so the
    // sheet shows what's locally known. When TaskEntity gains those
    // fields (migration 0008+), pull POD photo URL via /v1/pods/:id.
    var selectedCompletedTask by remember { mutableStateOf<TaskEntity?>(null) }

    val active = state.activeTasks
    val done = state.completedTasks

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(c.ground)
    ) {
        MoveScreenHeader(
            label = "Route · ${plural(active.size, "stop")} left",
            title = "Your circuit",
        )

        LazyColumn(
            modifier = Modifier.fillMaxSize(),
            contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 4.dp, bottom = 24.dp),
            verticalArrangement = Arrangement.spacedBy(CardGap)
        ) {
            item(key = "summary") { CircuitSummary(remaining = active.size, done = done.size) }

            if (onScanManifest != null && active.isNotEmpty()) {
                item(key = "manifest") {
                    MoveActionPanel(
                        kicker = "Before you roll",
                        title = "Scan the hub manifest",
                        body = "Clear every piece in hub mode before the first stop.",
                        icon = Icons.Filled.Inventory2,
                        onClick = onScanManifest,
                    )
                }
            }

            itemsIndexed(active, key = { _, task -> task.id }) { index, task ->
                StopCard(
                    task = task,
                    stopNumber = index + 1,
                    isNext = index == 0,
                    onNavigate = { onNavigateToStop(task.id) },
                    onReorder = { steps ->
                        val to = (index + steps).coerceIn(0, active.lastIndex)
                        if (to != index) viewModel.reorder(index, to)
                    }
                )
            }

            if (done.isNotEmpty()) {
                item(key = "completed-heading") {
                    MoveLabel("Completed · ${done.size}", Modifier.padding(top = 12.dp), dot = c.success)
                }
                itemsIndexed(done, key = { _, task -> task.id }) { _, task ->
                    // Completed tasks: tap opens a read-only detail sheet so
                    // drivers can review what they delivered without losing
                    // the immutable visual cue.
                    CompletedStopRow(task = task, onClick = { selectedCompletedTask = task })
                }
            }
        }
    }

    selectedCompletedTask?.let { task ->
        val podPhotoUrl by viewModel.podPhotoUrl.collectAsState()
        val uriHandler = LocalUriHandler.current

        // Fetch photo URL as soon as the sheet opens for a confirmed completed task.
        // Copy to local val first — cross-module property can't be smart-cast directly.
        val taskPodId = task.podId
        LaunchedEffect(taskPodId) {
            if (taskPodId != null) viewModel.loadPodPhoto(taskPodId)
        }

        ModalBottomSheet(
            onDismissRequest = { selectedCompletedTask = null },
            containerColor = c.surface,
            scrimColor = c.scrim,
            dragHandle = { BottomSheetDefaults.DragHandle(color = c.hairline) },
        ) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 20.dp, vertical = 8.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                val (outcome, outcomeColor) = outcomeOf(task.status, c)
                Text(outcome.uppercase(), color = outcomeColor, fontSize = 12.sp, fontWeight = FontWeight.Bold, letterSpacing = 2.4.sp)
                Text(task.recipientName, color = c.ink, fontFamily = Condensed, fontWeight = FontWeight.Bold, fontSize = 28.sp)
                Text(task.awb, color = c.accent, fontSize = 14.sp, fontFamily = FontFamily.Monospace)
                MoveDivider()
                MoveDetailRow("Phone", task.recipientPhone)
                MoveDetailRow("Address", task.address)
                if (task.isCod) MoveDetailRow("COD", "₱${task.codAmount.toInt()}")
                if (task.attemptCount > 0) MoveDetailRow("Attempts", task.attemptCount.toString())
                task.failureReason?.takeIf { it.isNotBlank() }?.let { MoveDetailRow("Reason", it, valueColor = c.amber) }
                Spacer(Modifier.height(4.dp))
                // POD photo — only available once the TASK_COMPLETE sync has confirmed
                // the server-side pod_id back to local DB. Tasks completed offline show
                // the fallback message until the next sync.
                val url = podPhotoUrl
                if (taskPodId != null) {
                    when {
                        url == null -> Row(
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(10.dp)
                        ) {
                            CircularProgressIndicator(color = c.accent, modifier = Modifier.size(18.dp), strokeWidth = 2.dp)
                            Text("Loading the POD photo…", color = c.muted, fontSize = 13.sp)
                        }
                        url.isEmpty() -> Text("POD captured — the photo is viewable in the admin portal.", color = c.muted, fontSize = 13.sp)
                        else -> MoveBigButton("VIEW POD PHOTO", onClick = { uriHandler.openUri(url) }, filled = false, height = 56.dp)
                    }
                } else {
                    Text("POD pending sync — the photo is available once it uploads.", color = c.muted, fontSize = 13.sp)
                }
                Spacer(Modifier.navigationBarsPadding().height(16.dp))
            }
        }
    }
}

@Composable
private fun CircuitSummary(remaining: Int, done: Int) {
    val c = LocalMoveColors.current
    MovePanel {
        Text(
            if (remaining > 1) "IN STOP ORDER · DRAG ≡ TO REORDER" else "STOP ORDER",
            color = c.accent,
            fontSize = 12.sp,
            letterSpacing = 2.4.sp,
        )
        Text(
            when {
                remaining == 0 && done == 0 -> "No stops on this shift yet"
                remaining == 0 -> "All ${plural(done, "stop")} done"
                done == 0 -> "${plural(remaining, "stop")} to run"
                else -> "${plural(remaining, "stop")} left, $done done"
            },
            color = c.ink,
            fontFamily = Condensed,
            fontWeight = FontWeight.Bold,
            fontSize = 32.sp,
            lineHeight = 34.sp,
            modifier = Modifier.padding(top = 8.dp),
        )
    }
}

@Composable
private fun StopCard(
    task: TaskEntity,
    stopNumber: Int,
    isNext: Boolean,
    onNavigate: () -> Unit,
    onReorder: (steps: Int) -> Unit,
) {
    val c = LocalMoveColors.current
    val density = LocalDensity.current
    var cardHeightPx by remember { mutableIntStateOf(0) }
    var dragAccumulator by remember { mutableFloatStateOf(0f) }
    val shape = RoundedCornerShape(20.dp)

    Row(
        modifier = Modifier
            .fillMaxWidth()
            .onSizeChanged { cardHeightPx = it.height }
            .clip(shape)
            .background(if (isNext) c.accentPanel else c.panel)
            .border(1.dp, if (isNext) c.accentBorder else c.hairline, shape)
            .padding(18.dp),
        horizontalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Box(
            modifier = Modifier
                .size(44.dp)
                .clip(RoundedCornerShape(14.dp))
                .background(if (isNext) c.accent else c.chip),
            contentAlignment = Alignment.Center
        ) {
            Text(
                "$stopNumber",
                color = if (isNext) c.accentInk else c.ink,
                fontFamily = Condensed,
                fontWeight = FontWeight.Bold,
                fontSize = 22.sp
            )
        }
        Column(Modifier.weight(1f)) {
            Row(verticalAlignment = Alignment.Top) {
                Column(Modifier.weight(1f)) {
                    Text(
                        task.recipientName,
                        color = c.ink,
                        fontFamily = Condensed,
                        fontWeight = FontWeight.Bold,
                        fontSize = 26.sp,
                        lineHeight = 28.sp,
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis,
                    )
                    liveStatusOf(task.status, c)?.let { (label, color) ->
                        Text(label.uppercase(), color = color, fontSize = 12.sp, fontWeight = FontWeight.Bold, letterSpacing = 1.4.sp)
                    }
                }
                // 56 dp drag target: reorder by dragging the handle up or down
                // by whole cards.
                Box(
                    modifier = Modifier
                        .size(56.dp)
                        .draggable(
                            state = rememberDraggableState { delta -> dragAccumulator += delta },
                            orientation = Orientation.Vertical,
                            onDragStarted = { dragAccumulator = 0f },
                            onDragStopped = {
                                val stepPx = cardHeightPx + with(density) { CardGap.toPx() }
                                val steps = if (stepPx > 0f) (dragAccumulator / stepPx).roundToInt() else 0
                                if (steps != 0) onReorder(steps)
                                dragAccumulator = 0f
                            }
                        ),
                    contentAlignment = Alignment.Center,
                ) {
                    Icon(Icons.Filled.DragHandle, contentDescription = "Drag to reorder", tint = c.muted, modifier = Modifier.size(26.dp))
                }
            }
            Text(task.address, color = c.ink, fontSize = 15.sp, maxLines = 2, overflow = TextOverflow.Ellipsis, modifier = Modifier.padding(top = 4.dp))
            Text(task.awb, color = c.muted, fontSize = 13.sp, fontFamily = FontFamily.Monospace, modifier = Modifier.padding(top = 3.dp))
            SyncBadge(task)
            Spacer(Modifier.height(14.dp))
            MoveBigButton(
                label = "NAVIGATE",
                onClick = onNavigate,
                filled = isNext,
                icon = Icons.Filled.Navigation,
                height = 56.dp,
            )
        }
    }
}

@Composable
private fun CompletedStopRow(task: TaskEntity, onClick: () -> Unit) {
    val c = LocalMoveColors.current
    val (outcome, color) = outcomeOf(task.status, c)
    val shape = RoundedCornerShape(18.dp)
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = 64.dp)
            .clip(shape)
            .background(c.panel)
            .border(1.dp, c.hairline, shape)
            .clickable(role = Role.Button, onClick = onClick)
            .padding(horizontal = 16.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Box(Modifier.size(10.dp).clip(CircleShape).background(color))
        Column(Modifier.weight(1f)) {
            Text(task.recipientName, color = c.ink, fontSize = 16.sp, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text("$outcome · ${task.awb}", color = c.muted, fontSize = 13.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
            SyncBadge(task)
        }
        Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, contentDescription = null, tint = c.muted)
    }
}

/** Sync state — only visible when a task is done locally but not yet confirmed by the backend. */
@Composable
private fun SyncBadge(task: TaskEntity) {
    val c = LocalMoveColors.current
    val badge: Pair<String, Color>? = when {
        // Reads the dedicated flag, not status — status now stays truthful
        // about the task's own lifecycle. Wording points at Retry because
        // that path can actually recover these now: it re-enqueues the
        // abandoned sync item instead of only resetting queue backoff.
        task.syncFailed -> "Sync failed — tap Retry on Home" to c.penalty
        task.status == TaskStatus.COMPLETED && !task.isSynced -> "Pending sync" to c.amber
        else -> null
    }
    badge?.let { (label, color) ->
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(6.dp),
            modifier = Modifier.padding(top = 4.dp),
        ) {
            Box(Modifier.size(8.dp).clip(CircleShape).background(color))
            Text(label, color = color, fontSize = 12.sp, fontWeight = FontWeight.SemiBold)
        }
    }
}

private fun liveStatusOf(status: TaskStatus, c: MoveColors): Pair<String, Color>? = when (status) {
    TaskStatus.EN_ROUTE -> "En route" to c.accent
    TaskStatus.ARRIVED -> "Arrived" to c.accent
    TaskStatus.IN_PROGRESS -> "In progress" to c.accent
    TaskStatus.ATTEMPTED, TaskStatus.FAILED -> "Attempted" to c.amber
    TaskStatus.FAILED_SYNC -> "Sync failed" to c.penalty
    else -> null
}

private fun outcomeOf(status: TaskStatus, c: MoveColors): Pair<String, Color> = when (status) {
    TaskStatus.COMPLETED -> "Done" to c.success
    TaskStatus.FAILED, TaskStatus.ATTEMPTED -> "Attempted" to c.amber
    TaskStatus.RETURNED -> "Returned" to c.amber
    else -> "Stop details" to c.muted
}

private fun plural(n: Int, word: String): String = "$n $word" + if (n == 1) "" else "s"
