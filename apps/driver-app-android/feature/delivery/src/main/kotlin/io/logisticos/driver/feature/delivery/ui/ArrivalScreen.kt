package io.logisticos.driver.feature.delivery.ui

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.slideInVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Phone
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.hilt.navigation.compose.hiltViewModel
import io.logisticos.driver.core.database.entity.TaskType
import io.logisticos.driver.feature.delivery.presentation.ArrivalViewModel

private val Canvas  = Color(0xFF050810)
private val Cyan    = Color(0xFF00E5FF)
private val Green   = Color(0xFF00FF88)
private val Amber   = Color(0xFFFFAB00)
private val Purple  = Color(0xFFA855F7)
private val Glass   = Color(0x0AFFFFFF)
private val Border  = Color(0x14FFFFFF)

@Composable
fun ArrivalScreen(
    taskId: String,
    onStartTask: (taskId: String, requiresPhoto: Boolean, requiresSignature: Boolean, requiresOtp: Boolean, isCod: Boolean, codAmount: Double) -> Unit,
    viewModel: ArrivalViewModel = hiltViewModel()
) {
    val state by viewModel.uiState.collectAsState()

    LaunchedEffect(taskId) { viewModel.load(taskId) }

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(Canvas),
        contentAlignment = Alignment.BottomCenter
    ) {
        // Background pulse ring
        Box(
            modifier = Modifier
                .align(Alignment.TopCenter)
                .padding(top = 80.dp)
                .size(160.dp)
                .clip(RoundedCornerShape(80.dp))
                .background(Cyan.copy(alpha = 0.04f))
                .border(1.dp, Cyan.copy(alpha = 0.12f), RoundedCornerShape(80.dp)),
            contentAlignment = Alignment.Center
        ) {
            Box(
                modifier = Modifier
                    .size(100.dp)
                    .clip(RoundedCornerShape(50.dp))
                    .background(Cyan.copy(alpha = 0.08f))
                    .border(1.dp, Cyan.copy(alpha = 0.25f), RoundedCornerShape(50.dp)),
                contentAlignment = Alignment.Center
            ) {
                Text("📍", fontSize = 36.sp)
            }
        }

        // Main card
        AnimatedVisibility(
            visible = state.task != null,
            enter = fadeIn() + slideInVertically { it / 2 }
        ) {
            val task = state.task ?: return@AnimatedVisibility

            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(topStart = 24.dp, topEnd = 24.dp))
                    .background(Color(0xFF0D1220))
                    .border(
                        width = 1.dp,
                        color = Border,
                        shape = RoundedCornerShape(topStart = 24.dp, topEnd = 24.dp)
                    )
                    .padding(horizontal = 20.dp, vertical = 24.dp),
                verticalArrangement = Arrangement.spacedBy(16.dp)
            ) {
                // Header
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Column {
                        Text(
                            when (task.taskType) {
                                TaskType.PICKUP   -> "Arrived at Pickup"
                                TaskType.RETURN   -> "Arrived at Hub"
                                TaskType.HUB_DROP -> "Arrived at Hub"
                                else              -> "Arrived at Stop"
                            },
                            color = Cyan,
                            fontSize = 13.sp,
                            fontWeight = FontWeight.SemiBold,
                            letterSpacing = 0.5.sp
                        )
                        Text(
                            task.recipientName,
                            color = Color.White,
                            fontSize = 22.sp,
                            fontWeight = FontWeight.Bold
                        )
                    }
                    // Task type badge
                    Box(
                        modifier = Modifier
                            .clip(RoundedCornerShape(8.dp))
                            .background(
                                when (task.taskType) {
                                    TaskType.PICKUP   -> Purple.copy(alpha = 0.12f)
                                    TaskType.RETURN   -> Amber.copy(alpha = 0.12f)
                                    TaskType.HUB_DROP -> Amber.copy(alpha = 0.12f)
                                    else              -> Cyan.copy(alpha = 0.10f)
                                }
                            )
                            .padding(horizontal = 10.dp, vertical = 4.dp)
                    ) {
                        Text(
                            when (task.taskType) {
                                TaskType.PICKUP   -> "PICKUP"
                                TaskType.RETURN   -> "RETURN"
                                TaskType.HUB_DROP -> "HUB DROP"
                                else              -> "DELIVERY"
                            },
                            color = when (task.taskType) {
                                TaskType.PICKUP   -> Purple
                                TaskType.RETURN   -> Amber
                                TaskType.HUB_DROP -> Amber
                                else              -> Cyan
                            },
                            fontSize = 10.sp,
                            fontWeight = FontWeight.Bold,
                            letterSpacing = 1.sp
                        )
                    }
                }

                HorizontalDivider(color = Border)

                // AWB
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .clip(RoundedCornerShape(10.dp))
                        .background(Glass)
                        .padding(12.dp),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Text("AWB", color = Color.White.copy(alpha = 0.4f), fontSize = 11.sp)
                    Text(
                        task.awb,
                        color = Color.White,
                        fontSize = 14.sp,
                        fontWeight = FontWeight.Medium,
                        fontFamily = FontFamily.Monospace
                    )
                }

                // Address
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(10.dp),
                    verticalAlignment = Alignment.Top
                ) {
                    Text("📍", fontSize = 16.sp, modifier = Modifier.padding(top = 2.dp))
                    Text(
                        task.address,
                        color = Color.White.copy(alpha = 0.8f),
                        fontSize = 14.sp,
                        lineHeight = 20.sp,
                        modifier = Modifier.weight(1f)
                    )
                }

                // Phone
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(10.dp),
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Icon(
                        Icons.Default.Phone,
                        contentDescription = null,
                        tint = Color.White.copy(alpha = 0.4f),
                        modifier = Modifier.size(16.dp)
                    )
                    Text(
                        task.recipientPhone,
                        color = Color.White.copy(alpha = 0.7f),
                        fontSize = 14.sp
                    )
                }

                // COD badge (only for deliveries with COD)
                if (task.isCod && task.taskType == TaskType.DELIVERY) {
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .clip(RoundedCornerShape(10.dp))
                            .background(Amber.copy(alpha = 0.10f))
                            .border(1.dp, Amber.copy(alpha = 0.20f), RoundedCornerShape(10.dp))
                            .padding(12.dp),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Text(
                            "💰 COD to Collect",
                            color = Amber,
                            fontSize = 13.sp,
                            fontWeight = FontWeight.SemiBold
                        )
                        Text(
                            "₱${"%,.2f".format(task.codAmount)}",
                            color = Amber,
                            fontSize = 18.sp,
                            fontWeight = FontWeight.Bold,
                            fontFamily = FontFamily.Monospace
                        )
                    }
                }

                // Notes
                val notes = task.notes
                if (!notes.isNullOrBlank()) {
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .clip(RoundedCornerShape(10.dp))
                            .background(Glass)
                            .padding(12.dp),
                        horizontalArrangement = Arrangement.spacedBy(8.dp)
                    ) {
                        Text("📝", fontSize = 14.sp)
                        Text(
                            notes,
                            color = Color.White.copy(alpha = 0.6f),
                            fontSize = 13.sp,
                            lineHeight = 18.sp
                        )
                    }
                }

                Spacer(Modifier.height(8.dp))

                // CTA button
                Button(
                    onClick = {
                        viewModel.startTask(taskId) {
                            onStartTask(
                                taskId,
                                task.requiresPhoto,
                                task.requiresSignature,
                                task.requiresOtp,
                                task.isCod,
                                task.codAmount
                            )
                        }
                    },
                    enabled = !state.isTransitioning,
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(56.dp),
                    shape = RoundedCornerShape(14.dp),
                    colors = ButtonDefaults.buttonColors(
                        containerColor = when (task.taskType) {
                            TaskType.PICKUP   -> Purple
                            TaskType.RETURN,
                            TaskType.HUB_DROP -> Amber
                            else              -> Cyan
                        }
                    )
                ) {
                    if (state.isTransitioning) {
                        CircularProgressIndicator(
                            color = Canvas,
                            modifier = Modifier.size(20.dp),
                            strokeWidth = 2.dp
                        )
                    } else {
                        Text(
                            when (task.taskType) {
                                TaskType.PICKUP   -> "Start Pickup"
                                TaskType.RETURN   -> "Start Hub Drop-off"
                                TaskType.HUB_DROP -> "Start Hub Drop-off"
                                else              -> "Start Delivery"
                            },
                            color = Canvas,
                            fontWeight = FontWeight.Bold,
                            fontSize = 16.sp
                        )
                    }
                }

                Spacer(Modifier.navigationBarsPadding())
            }
        }

        // Loading state
        if (state.task == null && !state.isTransitioning) {
            CircularProgressIndicator(
                color = Cyan,
                modifier = Modifier.align(Alignment.Center)
            )
        }
    }
}
