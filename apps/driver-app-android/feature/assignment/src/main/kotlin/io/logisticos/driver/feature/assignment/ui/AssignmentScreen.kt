package io.logisticos.driver.feature.assignment.ui

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.slideInVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.LocationOn
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
import io.logisticos.driver.core.common.AssignmentPayload
import io.logisticos.driver.feature.assignment.presentation.AssignmentViewModel

// ─── Design palette (matches ArrivalScreen dark glassmorphism theme) ──────────
private val Canvas = Color(0xFF050810)
private val Cyan   = Color(0xFF00E5FF)
private val Green  = Color(0xFF00FF88)
private val Amber  = Color(0xFFFFAB00)
private val Purple = Color(0xFFA855F7)
private val Glass  = Color(0x0AFFFFFF)
private val Border = Color(0x14FFFFFF)
private val Red    = Color(0xFFFF4D4D)

private val REJECT_REASONS = listOf(
    "ALREADY_BUSY"       to "Already on another delivery",
    "TOO_FAR"            to "Pickup too far away",
    "VEHICLE_ISSUE"      to "Vehicle issue",
    "PERSONAL_EMERGENCY" to "Personal emergency",
    "OTHER"              to "Other reason",
)

/**
 * Full-screen assignment card shown when dispatch assigns a new shipment to
 * this driver via FCM. The driver must explicitly Accept or Reject before
 * navigating elsewhere.
 *
 * [payload] is passed from [PendingAssignmentBus] via the nav graph.
 * [onAccepted] navigates to the Route tab (task is now in task list).
 * [onRejected] pops back to Home (assignment declined, re-dispatch triggered).
 */
@Composable
fun AssignmentScreen(
    payload: AssignmentPayload,
    onAccepted: () -> Unit,
    onRejected: () -> Unit,
    viewModel: AssignmentViewModel = hiltViewModel<AssignmentViewModel,
            AssignmentViewModel.Factory> { it.create(payload) }
) {
    val state by viewModel.uiState.collectAsState()

    // Track which action completed so we route correctly after isDone fires.
    var accepted by remember { mutableStateOf(false) }
    LaunchedEffect(state.isDone) {
        if (state.isDone) {
            if (accepted) onAccepted() else onRejected()
        }
    }

    if (state.showRejectSheet) {
        RejectReasonSheet(
            onDismiss = { viewModel.dismissRejectSheet() },
            onSelect  = { reason -> viewModel.reject(reason) }
        )
    }

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(Canvas),
        contentAlignment = Alignment.Center
    ) {
        // Concentric pulse rings behind the task-type icon
        Box(
            modifier = Modifier
                .align(Alignment.TopCenter)
                .padding(top = 72.dp)
                .size(140.dp)
                .clip(RoundedCornerShape(70.dp))
                .background(Cyan.copy(alpha = 0.05f))
                .border(1.dp, Cyan.copy(alpha = 0.15f), RoundedCornerShape(70.dp)),
            contentAlignment = Alignment.Center
        ) {
            Box(
                modifier = Modifier
                    .size(88.dp)
                    .clip(RoundedCornerShape(44.dp))
                    .background(Cyan.copy(alpha = 0.10f))
                    .border(1.dp, Cyan.copy(alpha = 0.30f), RoundedCornerShape(44.dp)),
                contentAlignment = Alignment.Center
            ) {
                Text(
                    text = if (state.taskType == "pickup") "📦" else "🚚",
                    fontSize = 32.sp
                )
            }
        }

        // Bottom sheet card — slides up on enter
        AnimatedVisibility(
            visible = true,
            enter = fadeIn() + slideInVertically { it / 2 },
            modifier = Modifier.align(Alignment.BottomCenter)
        ) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(topStart = 24.dp, topEnd = 24.dp))
                    .background(Color(0xFF0D1220))
                    .border(1.dp, Border, RoundedCornerShape(topStart = 24.dp, topEnd = 24.dp))
                    .padding(horizontal = 20.dp, vertical = 24.dp),
                verticalArrangement = Arrangement.spacedBy(16.dp)
            ) {
                // ── Header ────────────────────────────────────────────────────
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Column {
                        Text(
                            text = "New Assignment",
                            color = Cyan,
                            fontSize = 12.sp,
                            fontWeight = FontWeight.SemiBold,
                            letterSpacing = 0.8.sp
                        )
                        Text(
                            text = state.customerName,
                            color = Color.White,
                            fontSize = 22.sp,
                            fontWeight = FontWeight.Bold
                        )
                    }
                    Box(
                        modifier = Modifier
                            .clip(RoundedCornerShape(8.dp))
                            .background(
                                if (state.taskType == "pickup") Purple.copy(alpha = 0.12f)
                                else Cyan.copy(alpha = 0.10f)
                            )
                            .padding(horizontal = 10.dp, vertical = 4.dp)
                    ) {
                        Text(
                            text = state.taskType.uppercase(),
                            color = if (state.taskType == "pickup") Purple else Cyan,
                            fontSize = 10.sp,
                            fontWeight = FontWeight.Bold,
                            letterSpacing = 1.sp
                        )
                    }
                }

                HorizontalDivider(color = Border)

                // ── AWB row ───────────────────────────────────────────────────
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .clip(RoundedCornerShape(10.dp))
                        .background(Glass)
                        .padding(12.dp),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Text(
                        text = "AWB",
                        color = Color.White.copy(alpha = 0.4f),
                        fontSize = 11.sp
                    )
                    Text(
                        text = state.trackingNumber.ifBlank { state.shipmentId },
                        color = Color.White,
                        fontSize = 14.sp,
                        fontWeight = FontWeight.Medium,
                        fontFamily = FontFamily.Monospace
                    )
                }

                // ── Address ───────────────────────────────────────────────────
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(10.dp),
                    verticalAlignment = Alignment.Top
                ) {
                    Icon(
                        imageVector = Icons.Default.LocationOn,
                        contentDescription = null,
                        tint = Cyan.copy(alpha = 0.7f),
                        modifier = Modifier.size(18.dp).padding(top = 2.dp)
                    )
                    Text(
                        text = state.address,
                        color = Color.White.copy(alpha = 0.85f),
                        fontSize = 14.sp,
                        lineHeight = 20.sp,
                        modifier = Modifier.weight(1f)
                    )
                }

                // ── COD badge (delivery only, non-zero) ───────────────────────
                if (state.codAmountCents > 0 && state.taskType == "delivery") {
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
                            text = "💰 COD to Collect",
                            color = Amber,
                            fontSize = 13.sp,
                            fontWeight = FontWeight.SemiBold
                        )
                        Text(
                            text = "₱${"%.2f".format(state.codAmountCents / 100.0)}",
                            color = Amber,
                            fontSize = 18.sp,
                            fontWeight = FontWeight.Bold,
                            fontFamily = FontFamily.Monospace
                        )
                    }
                }

                // ── Error banner ──────────────────────────────────────────────
                state.error?.let { err ->
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .clip(RoundedCornerShape(10.dp))
                            .background(Red.copy(alpha = 0.10f))
                            .padding(12.dp)
                    ) {
                        Text(text = "⚠ $err", color = Red, fontSize = 13.sp)
                    }
                }

                Spacer(Modifier.height(4.dp))

                // ── Action buttons ────────────────────────────────────────────
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(12.dp)
                ) {
                    // Reject (outlined red)
                    OutlinedButton(
                        onClick = { viewModel.showRejectSheet() },
                        enabled = !state.isAccepting && !state.isRejecting,
                        modifier = Modifier.weight(1f).height(56.dp),
                        shape = RoundedCornerShape(14.dp),
                        border = androidx.compose.foundation.BorderStroke(1.dp, Red.copy(alpha = 0.5f)),
                        colors = ButtonDefaults.outlinedButtonColors(contentColor = Red)
                    ) {
                        if (state.isRejecting) {
                            CircularProgressIndicator(
                                color = Red,
                                modifier = Modifier.size(18.dp),
                                strokeWidth = 2.dp
                            )
                        } else {
                            Text("Reject", fontWeight = FontWeight.Bold, fontSize = 15.sp)
                        }
                    }

                    // Accept (green fill)
                    Button(
                        onClick = {
                            accepted = true
                            viewModel.accept()
                        },
                        enabled = !state.isAccepting && !state.isRejecting,
                        modifier = Modifier.weight(1f).height(56.dp),
                        shape = RoundedCornerShape(14.dp),
                        colors = ButtonDefaults.buttonColors(containerColor = Green)
                    ) {
                        if (state.isAccepting) {
                            CircularProgressIndicator(
                                color = Canvas,
                                modifier = Modifier.size(18.dp),
                                strokeWidth = 2.dp
                            )
                        } else {
                            Text(
                                text = "Accept",
                                color = Canvas,
                                fontWeight = FontWeight.Bold,
                                fontSize = 15.sp
                            )
                        }
                    }
                }

                Spacer(Modifier.navigationBarsPadding())
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun RejectReasonSheet(
    onDismiss: () -> Unit,
    onSelect:  (reason: String) -> Unit,
) {
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        containerColor   = Color(0xFF0D1220),
        tonalElevation   = 0.dp,
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 20.dp)
                .padding(bottom = 32.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp)
        ) {
            Text(
                text = "Reason for rejection",
                color = Color.White,
                fontSize = 17.sp,
                fontWeight = FontWeight.SemiBold,
                modifier = Modifier.padding(bottom = 8.dp)
            )
            REJECT_REASONS.forEach { (code, label) ->
                OutlinedButton(
                    onClick = { onSelect(code) },
                    modifier = Modifier.fillMaxWidth().height(52.dp),
                    shape = RoundedCornerShape(12.dp),
                    border = androidx.compose.foundation.BorderStroke(1.dp, Border),
                    colors = ButtonDefaults.outlinedButtonColors(contentColor = Color.White)
                ) {
                    Text(label, fontSize = 14.sp)
                }
            }
        }
    }
}
