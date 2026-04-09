package io.logisticos.driver.feature.route.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Menu
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.hilt.navigation.compose.hiltViewModel
import io.logisticos.driver.core.database.entity.TaskEntity
import io.logisticos.driver.core.database.entity.TaskStatus
import io.logisticos.driver.feature.route.presentation.RouteViewModel

private val Canvas = Color(0xFF050810)
private val Cyan = Color(0xFF00E5FF)
private val Green = Color(0xFF00FF88)
private val Amber = Color(0xFFFFAB00)
private val Glass = Color(0x0AFFFFFF)
private val Border = Color(0x14FFFFFF)

@Composable
fun RouteScreen(
    shiftId: String,
    onNavigateToStop: (taskId: String) -> Unit,
) {
    val viewModel: RouteViewModel = hiltViewModel(
        creationCallback = { factory: RouteViewModel.Factory -> factory.create(shiftId) }
    )
    val state by viewModel.uiState.collectAsState()

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
                    onClick = { onNavigateToStop(task.id) }
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
                    TaskStopCard(task = task, stopNumber = null, onClick = {})
                }
            }
        }
    }
}

@Composable
private fun TaskStopCard(task: TaskEntity, stopNumber: Int?, onClick: () -> Unit) {
    val statusColor = when (task.status) {
        TaskStatus.COMPLETED -> Green
        TaskStatus.ATTEMPTED, TaskStatus.FAILED -> Amber
        TaskStatus.EN_ROUTE, TaskStatus.ARRIVED, TaskStatus.IN_PROGRESS -> Cyan
        else -> Color.White.copy(alpha = 0.6f)
    }
    Card(
        onClick = onClick,
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(containerColor = Glass),
        border = androidx.compose.foundation.BorderStroke(1.dp, Border)
    ) {
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
            }
            if (stopNumber != null) {
                Icon(
                    Icons.Default.Menu,
                    contentDescription = "Drag",
                    tint = Color.White.copy(alpha = 0.3f)
                )
            }
        }
    }
}
