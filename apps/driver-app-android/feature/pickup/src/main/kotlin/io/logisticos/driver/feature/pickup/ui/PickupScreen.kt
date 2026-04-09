package io.logisticos.driver.feature.pickup.ui

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.QrCodeScanner
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
import io.logisticos.driver.feature.pickup.presentation.PickupViewModel

private val Canvas  = Color(0xFF050810)
private val Cyan    = Color(0xFF00E5FF)
private val Green   = Color(0xFF00FF88)
private val Amber   = Color(0xFFFFAB00)
private val Red     = Color(0xFFFF3B5C)
private val Purple  = Color(0xFFA855F7)
private val Glass   = Color(0x0AFFFFFF)
private val Border  = Color(0x14FFFFFF)

@Composable
fun PickupScreen(
    taskId: String,
    onCompleted: () -> Unit,
    viewModel: PickupViewModel = hiltViewModel()
) {
    val state by viewModel.uiState.collectAsState()

    LaunchedEffect(taskId) { viewModel.load(taskId) }

    // Success overlay
    if (state.isCompleted) {
        Box(
            modifier = Modifier
                .fillMaxSize()
                .background(Canvas),
            contentAlignment = Alignment.Center
        ) {
            Column(
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(16.dp)
            ) {
                Box(
                    modifier = Modifier
                        .size(80.dp)
                        .clip(RoundedCornerShape(40.dp))
                        .background(Green.copy(alpha = 0.12f))
                        .border(2.dp, Green.copy(alpha = 0.4f), RoundedCornerShape(40.dp)),
                    contentAlignment = Alignment.Center
                ) {
                    Icon(Icons.Default.Check, contentDescription = null, tint = Green, modifier = Modifier.size(40.dp))
                }
                Text("Pickup Confirmed", color = Green, fontSize = 22.sp, fontWeight = FontWeight.Bold)
                Text("Parcel collected successfully", color = Color.White.copy(alpha = 0.5f), fontSize = 14.sp)
                Button(
                    onClick = onCompleted,
                    modifier = Modifier.padding(top = 8.dp).width(200.dp).height(48.dp),
                    colors = ButtonDefaults.buttonColors(containerColor = Green),
                    shape = RoundedCornerShape(12.dp)
                ) {
                    Text("Continue", color = Canvas, fontWeight = FontWeight.Bold)
                }
            }
        }
        return
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(Canvas)
            .verticalScroll(rememberScrollState())
    ) {
        // Header
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 20.dp, vertical = 20.dp),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically
        ) {
            Column {
                Text("FIRST MILE", color = Purple, fontSize = 11.sp, fontWeight = FontWeight.Bold, letterSpacing = 1.sp)
                Text("Pickup Confirmation", color = Color.White, fontSize = 20.sp, fontWeight = FontWeight.Bold)
            }
            Box(
                modifier = Modifier
                    .clip(RoundedCornerShape(8.dp))
                    .background(Purple.copy(alpha = 0.12f))
                    .padding(horizontal = 10.dp, vertical = 4.dp)
            ) {
                Text("PICKUP", color = Purple, fontSize = 10.sp, fontWeight = FontWeight.Bold, letterSpacing = 1.sp)
            }
        }

        val task = state.task
        if (task == null) {
            Box(Modifier.fillMaxWidth().height(200.dp), contentAlignment = Alignment.Center) {
                CircularProgressIndicator(color = Cyan)
            }
            return@Column
        }

        // Merchant info card
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 20.dp)
                .clip(RoundedCornerShape(14.dp))
                .background(Glass)
                .border(1.dp, Border, RoundedCornerShape(14.dp))
                .padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp)
        ) {
            Text("Merchant / Sender", color = Color.White.copy(alpha = 0.4f), fontSize = 11.sp, letterSpacing = 0.5.sp)
            Text(task.recipientName, color = Color.White, fontSize = 18.sp, fontWeight = FontWeight.Bold)
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text("📍", fontSize = 14.sp)
                Text(task.address, color = Color.White.copy(alpha = 0.7f), fontSize = 13.sp, lineHeight = 18.sp)
            }
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text("📞", fontSize = 14.sp)
                Text(task.recipientPhone, color = Color.White.copy(alpha = 0.7f), fontSize = 13.sp)
            }
        }

        Spacer(Modifier.height(16.dp))

        // AWB scan section
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 20.dp)
                .clip(RoundedCornerShape(14.dp))
                .background(Glass)
                .border(
                    1.dp,
                    when {
                        state.awbMismatch -> Red.copy(alpha = 0.4f)
                        state.awbScanned  -> Green.copy(alpha = 0.4f)
                        else              -> Border
                    },
                    RoundedCornerShape(14.dp)
                )
                .padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp)
        ) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically
            ) {
                Text("AWB Verification", color = Color.White.copy(alpha = 0.6f), fontSize = 12.sp)
                AnimatedVisibility(visible = state.awbScanned, enter = fadeIn(), exit = fadeOut()) {
                    Icon(Icons.Default.Check, contentDescription = null, tint = Green, modifier = Modifier.size(18.dp))
                }
                AnimatedVisibility(visible = state.awbMismatch, enter = fadeIn(), exit = fadeOut()) {
                    Icon(Icons.Default.Close, contentDescription = null, tint = Red, modifier = Modifier.size(18.dp))
                }
            }

            // Expected AWB display
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween
            ) {
                Text("Expected", color = Color.White.copy(alpha = 0.4f), fontSize = 12.sp)
                Text(
                    task.awb,
                    color = Color.White,
                    fontSize = 14.sp,
                    fontWeight = FontWeight.SemiBold,
                    fontFamily = FontFamily.Monospace
                )
            }

            if (state.scannedAwb.isNotEmpty()) {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween
                ) {
                    Text("Scanned", color = Color.White.copy(alpha = 0.4f), fontSize = 12.sp)
                    Text(
                        state.scannedAwb,
                        color = if (state.awbMismatch) Red else Green,
                        fontSize = 14.sp,
                        fontWeight = FontWeight.SemiBold,
                        fontFamily = FontFamily.Monospace
                    )
                }
                if (state.awbMismatch) {
                    Text(
                        "AWB does not match. Scan the correct barcode.",
                        color = Red,
                        fontSize = 12.sp
                    )
                }
            }

            // Manual AWB entry as fallback
            var manualEntry by remember { mutableStateOf("") }
            OutlinedTextField(
                value = manualEntry,
                onValueChange = { manualEntry = it },
                label = { Text("Enter AWB manually") },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
                trailingIcon = {
                    IconButton(onClick = { if (manualEntry.isNotBlank()) viewModel.onAwbScanned(manualEntry) }) {
                        Icon(Icons.Default.QrCodeScanner, contentDescription = null, tint = Cyan)
                    }
                },
                colors = OutlinedTextFieldDefaults.colors(
                    focusedBorderColor = Cyan,
                    unfocusedBorderColor = Border,
                    focusedTextColor = Color.White,
                    unfocusedTextColor = Color.White,
                    focusedLabelColor = Cyan,
                    unfocusedLabelColor = Color.White.copy(alpha = 0.4f),
                    cursorColor = Cyan
                )
            )
        }

        Spacer(Modifier.height(16.dp))

        // Photo section
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 20.dp)
                .clip(RoundedCornerShape(14.dp))
                .background(Glass)
                .border(
                    1.dp,
                    if (state.photoPath != null) Green.copy(alpha = 0.3f) else Border,
                    RoundedCornerShape(14.dp)
                )
                .padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp)
        ) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically
            ) {
                Text("Parcel Photo", color = Color.White.copy(alpha = 0.6f), fontSize = 12.sp)
                Text(
                    if (state.photoPath != null) "Captured ✓" else "Optional",
                    color = if (state.photoPath != null) Green else Color.White.copy(alpha = 0.3f),
                    fontSize = 11.sp
                )
            }

            // Photo placeholder — CameraX handled in PodScreen; here we use the same saveBitmap pattern
            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .height(120.dp)
                    .clip(RoundedCornerShape(10.dp))
                    .background(Color(0x08FFFFFF))
                    .border(1.dp, Border, RoundedCornerShape(10.dp)),
                contentAlignment = Alignment.Center
            ) {
                if (state.photoPath != null) {
                    Text("📷  Photo captured", color = Green, fontSize = 14.sp)
                } else {
                    Column(
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.spacedBy(4.dp)
                    ) {
                        Text("📷", fontSize = 28.sp)
                        Text("Tap camera to capture parcel photo", color = Color.White.copy(alpha = 0.3f), fontSize = 12.sp)
                    }
                }
            }
        }

        Spacer(Modifier.height(24.dp))

        // Confirm button
        Button(
            onClick = { viewModel.confirmPickup(taskId, onCompleted) },
            enabled = state.canConfirm && !state.isConfirming,
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 20.dp)
                .height(56.dp),
            shape = RoundedCornerShape(14.dp),
            colors = ButtonDefaults.buttonColors(
                containerColor = Green,
                disabledContainerColor = Color.White.copy(alpha = 0.08f)
            )
        ) {
            if (state.isConfirming) {
                CircularProgressIndicator(color = Canvas, modifier = Modifier.size(20.dp), strokeWidth = 2.dp)
            } else {
                Text(
                    "Confirm Pickup",
                    color = if (state.canConfirm) Canvas else Color.White.copy(alpha = 0.3f),
                    fontWeight = FontWeight.Bold,
                    fontSize = 16.sp
                )
            }
        }

        if (!state.awbScanned) {
            Text(
                "Scan or enter the AWB barcode to enable confirmation",
                color = Color.White.copy(alpha = 0.3f),
                fontSize = 12.sp,
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 20.dp, vertical = 8.dp),
                textAlign = androidx.compose.ui.text.style.TextAlign.Center
            )
        }

        Spacer(Modifier.navigationBarsPadding().height(16.dp))
    }
}
