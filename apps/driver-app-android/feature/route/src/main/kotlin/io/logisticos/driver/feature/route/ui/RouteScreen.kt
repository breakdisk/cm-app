package io.logisticos.driver.feature.route.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.draggable
import androidx.compose.foundation.gestures.rememberDraggableState
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Menu
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.hilt.navigation.compose.hiltViewModel
import io.logisticos.driver.core.database.entity.TaskEntity
import io.logisticos.driver.core.database.entity.TaskStatus
import io.logisticos.driver.feature.route.presentation.RouteViewModel
import kotlin.math.roundToInt

private val Canvas = Color(0xFF050810)
private val Cyan = Color(0xFF00E5FF)
private val Green = Color(0xFF00FF88)
private val Amber = Color(0xFFFFAB00)
private val Red = Color(0xFFFF4444)
private val Glass = Color(0x0AFFFFFF)
private val Border = Color(0x14FFFFFF)

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun RouteScreen(
    shiftId: String,
    onNavigateToStop: (taskId: String) -> Unit,
) {
    val viewModel: RouteViewModel = hiltViewModel(
        creationCallback = { factory: RouteViewModel.Factory -> factory.create(shiftId) }
    )
    val state by viewModel.uiState.collectAsState()

    // Selected completed task for the detail bottom-sheet. Read-only —
    // we don't yet store completed_at / pod_id on TaskEntity, so the
    // sheet shows what's locally known. When TaskEntity gains those
    // fields (migration 0008+), pull POD photo URL via /v1/pods/:id.
    var selectedCompletedTask by remember { mutableStateOf<TaskEntity?>(null) }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(Canvas)
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(16.dp),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically
        ) {
            Text("Route", color = Color.White, fontSize = 22.sp, fontWeight = FontWeight.Bold)
            Text(
                "${state.activeTasks.size} stops remaining",
                color = Color.White.copy(alpha = 0.5f),
                fontSize = 13.sp
            )
        }

        LazyColumn(
            modifier = Modifier.fillMaxSize(),
            contentPadding = PaddingValues(horizontal = 16.dp, vertical = 8.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp)
        ) {
            itemsIndexed(state.activeTasks, key = { _, task -> task.id }) { index, task ->
                TaskStopCard(
                    task = task,
                    stopNumber = index + 1,
                    onClick = { onNavigateToStop(task.id) },
                    onReorder = { steps ->
                        val to = (index + steps).coerceIn(0, state.activeTasks.lastIndex)
                        if (to != index) viewModel.reorder(index, to)
                    }
                )
            }

            if (state.completedTasks.isNotEmpty()) {
                item {
                    Text(
                        "Completed (${state.completedTasks.size})",
                        color = Color.White.copy(alpha = 0.4f),
                        fontSize = 13.sp,
                        modifier = Modifier.padding(top = 16.dp, bottom = 4.dp)
                    )
                }
                itemsIndexed(state.completedTasks, key = { _, task -> task.id }) { _, task ->
                    // Completed tasks: tap opens a read-only detail sheet so
                    // drivers can review what they delivered without losing
                    // the immutable visual cue.
                    TaskStopCard(
                        task = task,
                        stopNumber = null,
                        onClick = { selectedCompletedTask = task },
                    )
                }
            }
        }
    }

    selectedCompletedTask?.let { task ->
        val podPhotoUrl by viewModel.podPhotoUrl.collectAsState()
        val uriHandler = LocalUriHandler.current

        // Fetch photo URL as soon as the sheet opens for a confirmed completed task
        LaunchedEffect(task.podId) {
            if (task.podId != null) viewModel.loadPodPhoto(task.podId)
        }

        ModalBottomSheet(
            onDismissRequest = { selectedCompletedTask = null },
            containerColor = Color(0xFF0A0E1A),
        ) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 24.dp, vertical = 16.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Text(
                    when (task.status) {
                        TaskStatus.COMPLETED -> "Delivery completed"
                        TaskStatus.FAILED, TaskStatus.ATTEMPTED -> "Delivery attempted"
                        TaskStatus.RETURNED -> "Returned"
                        else -> "Stop details"
                    },
                    color = when (task.status) {
                        TaskStatus.COMPLETED -> Green
                        TaskStatus.FAILED, TaskStatus.ATTEMPTED, TaskStatus.RETURNED -> Amber
                        else -> Color.White
                    },
                    fontWeight = FontWeight.Bold,
                    fontSize = 16.sp,
                )
                Text(task.awb, color = Cyan, fontSize = 13.sp, fontWeight = FontWeight.Medium)
                DetailRow("Recipient", task.recipientName)
                DetailRow("Phone", task.recipientPhone)
                DetailRow("Address", task.address)
                if (task.isCod) DetailRow("COD", "₱${task.codAmount.toInt()}")
                if (task.attemptCount > 0) {
                    DetailRow("Attempts", task.attemptCount.toString())
                }
                if (!task.failureReason.isNullOrBlank()) {
                    DetailRow("Reason", task.failureReason!!, valueColor = Amber)
                }
                Spacer(Modifier.height(4.dp))
                // POD photo — only available once the TASK_COMPLETE sync has confirmed
                // the server-side pod_id back to local DB. Tasks completed offline show
                // the fallback message until the next sync.
                if (task.podId != null) {
                    when (podPhotoUrl) {
                        null -> {
                            Row(
                                verticalAlignment = Alignment.CenterVertically,
                                horizontalArrangement = Arrangement.spacedBy(8.dp)
                            ) {
                                CircularProgressIndicator(
                                    color = Cyan,
                                    modifier = Modifier.size(14.dp),
                                    strokeWidth = 2.dp
                                )
                                Text("Loading POD photo…", color = Color.White.copy(alpha = 0.4f), fontSize = 11.sp)
                            }
                        }
                        "" -> Text(
                            "POD captured — photo viewable in the admin portal.",
                            color = Color.White.copy(alpha = 0.4f),
                            fontSize = 11.sp,
                        )
                        else -> Button(
                            onClick = { uriHandler.openUri(podPhotoUrl!!) },
                            modifier = Modifier.fillMaxWidth().height(40.dp),
                            shape = RoundedCornerShape(10.dp),
                            colors = ButtonDefaults.buttonColors(containerColor = Cyan.copy(alpha = 0.15f))
                        ) {
                            Text("View POD Photo", color = Cyan, fontSize = 13.sp, fontWeight = FontWeight.SemiBold)
                        }
                    }
                } else {
                    Text(
                        "POD pending sync — photo will be available once uploaded.",
                        color = Color.White.copy(alpha = 0.4f),
                        fontSize = 11.sp,
                    )
                }
                Spacer(Modifier.navigationBarsPadding().height(16.dp))
            }
        }
    }
}

@Composable
private fun DetailRow(label: String, value: String, valueColor: Color = Color.White) {
    Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Text(
            label,
            color = Color.White.copy(alpha = 0.4f),
            fontSize = 12.sp,
            modifier = Modifier.width(90.dp),
        )
        Text(value, color = valueColor, fontSize = 12.sp)
    }
}

// Approximate card height in dp used to compute reorder step from drag distance.
private const val CARD_HEIGHT_DP = 80

@Composable
private fun TaskStopCard(
    task: TaskEntity,
    stopNumber: Int?,
    onClick: (() -> Unit)?,
    onReorder: ((steps: Int) -> Unit)? = null,
) {
    val statusColor = when (task.status) {
        TaskStatus.COMPLETED -> Green
        TaskStatus.ATTEMPTED, TaskStatus.FAILED -> Amber
        TaskStatus.EN_ROUTE, TaskStatus.ARRIVED, TaskStatus.IN_PROGRESS -> Cyan
        TaskStatus.FAILED_SYNC -> Red
        else -> Color.White.copy(alpha = 0.6f)
    }
    // When onClick is null, use the non-clickable Card overload — completed
    // tasks shouldn't render with a ripple on tap. Material 3 has two
    // separate Card composables, so we branch here.
    if (onClick != null) {
        Card(
            onClick = onClick,
            modifier = Modifier.fillMaxWidth(),
            colors = CardDefaults.cardColors(containerColor = Glass),
            border = androidx.compose.foundation.BorderStroke(1.dp, Border)
        ) {
            TaskStopCardBody(task, stopNumber, statusColor)
        }
    } else {
        Card(
            modifier = Modifier.fillMaxWidth(),
            colors = CardDefaults.cardColors(containerColor = Glass.copy(alpha = 0.5f)),
            border = androidx.compose.foundation.BorderStroke(1.dp, Border)
        ) {
            TaskStopCardBody(task, stopNumber, statusColor)
        }
    }
}

@Composable
private fun TaskStopCardBody(task: TaskEntity, stopNumber: Int?, statusColor: Color) {
    Row(
        modifier = Modifier.padding(16.dp),
        horizontalArrangement = Arrangement.spacedBy(12.dp),
        verticalAlignment = Alignment.CenterVertically
    ) {
        if (stopNumber != null) {
            Box(
                modifier = Modifier
                    .size(32.dp)
                    .background(Cyan.copy(alpha = 0.15f), shape = MaterialTheme.shapes.small),
                contentAlignment = Alignment.Center
            ) {
                Text(
                    "$stopNumber",
                    color = Cyan,
                    fontWeight = FontWeight.Bold,
                    fontSize = 14.sp
                )
            }
        }
        Column(
            modifier = Modifier.weight(1f),
            verticalArrangement = Arrangement.spacedBy(2.dp)
        ) {
            Text(
                task.recipientName,
                color = Color.White,
                fontWeight = FontWeight.Medium,
                fontSize = 15.sp
            )
            Text(
                task.address,
                color = Color.White.copy(alpha = 0.5f),
                fontSize = 12.sp,
                maxLines = 1
            )
            Text(task.awb, color = statusColor, fontSize = 11.sp)
            // Sync state badge — only visible when task is done locally but not yet confirmed by backend.
            val syncBadgeColor: Color?
            val syncBadgeLabel: String?
            when {
                task.status == TaskStatus.FAILED_SYNC -> {
                    syncBadgeColor = Red
                    syncBadgeLabel = "Sync failed — contact support"
                }
                task.status == TaskStatus.COMPLETED && !task.isSynced -> {
                    syncBadgeColor = Amber
                    syncBadgeLabel = "Pending sync"
                }
                else -> {
                    syncBadgeColor = null
                    syncBadgeLabel = null
                }
            }
            if (syncBadgeColor != null && syncBadgeLabel != null) {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(4.dp),
                    modifier = Modifier.padding(top = 2.dp),
                ) {
                    Box(
                        modifier = Modifier
                            .size(6.dp)
                            .background(syncBadgeColor, shape = CircleShape)
                    )
                    Text(syncBadgeLabel, color = syncBadgeColor, fontSize = 10.sp)
                }
            }
        }
        if (stopNumber != null && onReorder != null) {
            val density = androidx.compose.ui.platform.LocalDensity.current
            var dragAccumulator by remember { mutableFloatStateOf(0f) }
            Icon(
                Icons.Default.Menu,
                contentDescription = "Drag to reorder",
                tint = Color.White.copy(alpha = 0.3f),
                modifier = Modifier.draggable(
                    state = rememberDraggableState { delta -> dragAccumulator += delta },
                    orientation = Orientation.Vertical,
                    onDragStarted = { dragAccumulator = 0f },
                    onDragStopped = {
                        val cardHeightPx = with(density) { CARD_HEIGHT_DP.dp.toPx() }
                        val steps = (dragAccumulator / cardHeightPx).roundToInt()
                        if (steps != 0) onReorder(steps)
                        dragAccumulator = 0f
                    }
                )
            )
        } else if (stopNumber != null) {
            Icon(
                Icons.Default.Menu,
                contentDescription = "Drag to reorder",
                tint = Color.White.copy(alpha = 0.3f)
            )
        }
    }
}
