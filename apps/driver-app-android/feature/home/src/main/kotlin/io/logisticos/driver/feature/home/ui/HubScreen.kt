package io.logisticos.driver.feature.home.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.hilt.navigation.compose.hiltViewModel
import io.logisticos.driver.core.database.entity.TaskEntity
import io.logisticos.driver.core.database.entity.TaskType
import io.logisticos.driver.feature.home.presentation.HubViewModel

private val Canvas = Color(0xFF050810)
private val Amber = Color(0xFFFFAB00)        // Hub uses amber accent (matches Arrival/Pickup hub styling)
private val Glass = Color(0x0AFFFFFF)
private val Border = Color(0x14FFFFFF)

/**
 * Hub Operations screen — lists the driver's open HUB_DROP and RETURN tasks
 * in stop-order. Tapping a task hands off to the existing arrival → pickup
 * confirmation flow (the same screen that already handles HUB_DROP).
 *
 * Rationale: drivers running a hub-drop circuit don't want to scroll past
 * doorstep deliveries on the regular route screen. One-tap entry to the
 * hub work-list keeps the sortation flow tight.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun HubScreen(
    onSelectTask: (taskId: String) -> Unit,
    onBack: () -> Unit,
    viewModel: HubViewModel = hiltViewModel(),
) {
    val state by viewModel.uiState.collectAsState()

    Scaffold(
        containerColor = Canvas,
        topBar = {
            TopAppBar(
                title = {
                    Text(
                        "Hub Operations",
                        color = Color.White,
                        fontWeight = FontWeight.SemiBold,
                    )
                },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(
                            Icons.AutoMirrored.Filled.ArrowBack,
                            contentDescription = "Back",
                            tint = Color.White,
                        )
                    }
                },
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = Canvas,
                    titleContentColor = Color.White,
                ),
            )
        },
    ) { inner ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(inner)
                .padding(horizontal = 16.dp, vertical = 12.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Text(
                "Pending hub drop-offs and returns. Tap a stop to confirm at the dock.",
                color = Color.White.copy(alpha = 0.55f),
                fontSize = 12.sp,
            )

            when {
                state.isLoading -> {
                    Box(modifier = Modifier.fillMaxWidth().padding(top = 32.dp), contentAlignment = Alignment.Center) {
                        CircularProgressIndicator(color = Amber)
                    }
                }
                state.hubTasks.isEmpty() -> {
                    EmptyHubState()
                }
                else -> {
                    LazyColumn(
                        verticalArrangement = Arrangement.spacedBy(10.dp),
                        modifier = Modifier.fillMaxSize(),
                    ) {
                        items(state.hubTasks, key = { it.id }) { task ->
                            HubTaskCard(task = task, onClick = { onSelectTask(task.id) })
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun HubTaskCard(task: TaskEntity, onClick: () -> Unit) {
    Card(
        modifier = Modifier
            .fillMaxWidth()
            .clickable { onClick() },
        colors = CardDefaults.cardColors(containerColor = Glass),
        border = androidx.compose.foundation.BorderStroke(1.dp, Border),
        shape = RoundedCornerShape(12.dp),
    ) {
        Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Box(
                    modifier = Modifier
                        .background(Amber.copy(alpha = 0.18f), RoundedCornerShape(6.dp))
                        .padding(horizontal = 6.dp, vertical = 2.dp),
                ) {
                    Text(
                        text = if (task.taskType == TaskType.HUB_DROP) "HUB DROP" else "RETURN",
                        color = Amber,
                        fontSize = 10.sp,
                        fontWeight = FontWeight.Bold,
                    )
                }
                Text(
                    text = "Stop ${task.stopOrder}",
                    color = Color.White.copy(alpha = 0.5f),
                    fontSize = 11.sp,
                )
            }
            Text(
                text = task.awb,
                color = Color.White,
                fontFamily = FontFamily.Monospace,
                fontSize = 14.sp,
                fontWeight = FontWeight.SemiBold,
            )
            Text(
                text = task.address,
                color = Color.White.copy(alpha = 0.7f),
                fontSize = 12.sp,
            )
        }
    }
}

@Composable
private fun EmptyHubState() {
    Box(
        modifier = Modifier
            .fillMaxWidth()
            .padding(top = 48.dp),
        contentAlignment = Alignment.Center,
    ) {
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text("🏢", fontSize = 36.sp)
            Text(
                "No pending hub drop-offs",
                color = Color.White.copy(alpha = 0.7f),
                fontSize = 14.sp,
                fontWeight = FontWeight.Medium,
            )
            Text(
                "Returns and HUB_DROP tasks for this shift will appear here.",
                color = Color.White.copy(alpha = 0.45f),
                fontSize = 12.sp,
            )
        }
    }
}
