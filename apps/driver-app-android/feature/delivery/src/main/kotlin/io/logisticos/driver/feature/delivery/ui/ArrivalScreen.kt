package io.logisticos.driver.feature.delivery.ui

import android.content.Intent
import android.net.Uri
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.Chat
import androidx.compose.material.icons.filled.Phone
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.hilt.navigation.compose.hiltViewModel
import io.logisticos.driver.core.database.entity.TaskType
import io.logisticos.driver.core.designsystem.*
import io.logisticos.driver.feature.delivery.presentation.ArrivalViewModel

/**
 * At the stop: who, where, what to collect, and the one button that starts the
 * handover. Pickups and hub drops go on to PickupScreen, deliveries to PodScreen.
 */
@Composable
fun ArrivalScreen(
    taskId: String,
    /** Carries the taskType so the nav graph can route pickups to PickupScreen
     *  and deliveries (or returns / hub-drops) to PodScreen. Without this
     *  branch, pickup tasks landed on PodScreen with all requires-* false
     *  and the driver saw a screen with no capture UI — the "no POD prompt"
     *  bug operations reported. */
    onStartTask: (taskId: String, taskType: TaskType, requiresPhoto: Boolean, requiresSignature: Boolean, requiresOtp: Boolean, isCod: Boolean, codAmount: Double) -> Unit,
    /** Opens the thread with the customer on this job; hidden when null. */
    onOpenChat: ((shipmentId: String, customerName: String, customerPhone: String) -> Unit)? = null,
    onBack: () -> Unit = {},
    viewModel: ArrivalViewModel = hiltViewModel()
) {
    val state by viewModel.uiState.collectAsState()
    val context = LocalContext.current
    val c = LocalMoveColors.current

    LaunchedEffect(taskId) { viewModel.load(taskId) }

    val task = state.task

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(c.ground)
            .verticalScroll(rememberScrollState())
    ) {
        // Back affordance — drivers who tap the wrong stop must always have
        // a way out.
        MoveScreenHeader(
            label = when (task?.taskType) {
                null              -> "Arriving"
                TaskType.PICKUP   -> "Arrived · pickup"
                TaskType.RETURN,
                TaskType.HUB_DROP -> "Arrived · hub"
                else              -> "Arrived · stop"
            },
            title = task?.recipientName ?: "Loading the stop",
            onBack = onBack,
        )

        if (task == null) {
            if (!state.isTransitioning) {
                Box(Modifier.fillMaxWidth().padding(top = 64.dp), contentAlignment = Alignment.Center) {
                    CircularProgressIndicator(color = c.accent)
                }
            }
            return@Column
        }

        val tone = when (task.taskType) {
            TaskType.RETURN, TaskType.HUB_DROP -> MoveTone.Amber
            else -> MoveTone.Accent
        }

        Column(
            modifier = Modifier.padding(horizontal = 16.dp),
            verticalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            MovePill(
                text = when (task.taskType) {
                    TaskType.PICKUP   -> "Pickup"
                    TaskType.RETURN   -> "Return"
                    TaskType.HUB_DROP -> "Hub drop"
                    else              -> "Delivery"
                },
                fg = c.tone(tone),
                bg = c.tonePanel(tone),
            )

            MovePanel {
                Text("AWB", color = c.muted, fontSize = 12.sp, letterSpacing = 2.4.sp)
                Text(task.awb, color = c.ink, fontSize = 20.sp, fontWeight = FontWeight.Medium, fontFamily = FontFamily.Monospace, modifier = Modifier.padding(top = 4.dp))
                MoveDivider(Modifier.padding(vertical = 14.dp))
                Text("ADDRESS", color = c.muted, fontSize = 12.sp, letterSpacing = 2.4.sp)
                Text(task.address, color = c.ink, fontSize = 17.sp, lineHeight = 24.sp, modifier = Modifier.padding(top = 4.dp))
            }

            // The thread with the customer. Both sides keep it on the job.
            if (onOpenChat != null && task.shipmentId.isNotBlank()) {
                MoveBigButton(
                    label = "MESSAGE THE CUSTOMER",
                    onClick = { onOpenChat(task.shipmentId, task.recipientName, task.recipientPhone) },
                    filled = false,
                    icon = Icons.AutoMirrored.Filled.Chat,
                    height = 60.dp,
                )
            }

            // Phone — calls the recipient directly from this screen.
            if (task.recipientPhone.isNotBlank()) {
                MoveBigButton(
                    label = "CALL ${task.recipientPhone}",
                    onClick = {
                        context.startActivity(Intent(Intent.ACTION_DIAL, Uri.parse("tel:${task.recipientPhone}")))
                    },
                    filled = false,
                    icon = Icons.Filled.Phone,
                    height = 60.dp,
                )
            }

            // COD (only for deliveries with COD)
            if (task.isCod && task.taskType == TaskType.DELIVERY) {
                MovePanel(tone = MoveTone.Amber) {
                    Text("CASH TO COLLECT", color = c.amber, fontSize = 12.sp, letterSpacing = 2.4.sp)
                    Text("₱${"%,.2f".format(task.codAmount)}", color = c.amber, fontFamily = Condensed, fontWeight = FontWeight.Bold, fontSize = 36.sp)
                }
            }

            task.notes?.takeIf { it.isNotBlank() }?.let { notes ->
                MovePanel {
                    Text("NOTES", color = c.muted, fontSize = 12.sp, letterSpacing = 2.4.sp)
                    Text(notes, color = c.ink, fontSize = 15.sp, lineHeight = 21.sp, modifier = Modifier.padding(top = 4.dp))
                }
            }

            MoveBigButton(
                label = when (task.taskType) {
                    TaskType.PICKUP   -> "START PICKUP"
                    TaskType.RETURN,
                    TaskType.HUB_DROP -> "START HUB DROP-OFF"
                    else              -> "START DELIVERY"
                },
                onClick = {
                    viewModel.startTask(taskId) {
                        onStartTask(
                            taskId,
                            task.taskType,
                            task.requiresPhoto,
                            task.requiresSignature,
                            task.requiresOtp,
                            task.isCod,
                            task.codAmount
                        )
                    }
                },
                tone = tone,
                loading = state.isTransitioning,
            )
        }

        Spacer(Modifier.navigationBarsPadding().height(24.dp))
    }
}
