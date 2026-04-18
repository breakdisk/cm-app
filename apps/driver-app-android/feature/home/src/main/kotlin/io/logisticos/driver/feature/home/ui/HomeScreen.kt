package io.logisticos.driver.feature.home.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.hilt.navigation.compose.hiltViewModel
import io.logisticos.driver.feature.home.presentation.HomeViewModel

private val Canvas = Color(0xFF050810)
private val Cyan = Color(0xFF00E5FF)
private val Amber = Color(0xFFFFAB00)
private val Green = Color(0xFF00FF88)
private val Glass = Color(0x0AFFFFFF)
private val Border = Color(0x14FFFFFF)

@Composable
fun HomeScreen(
    onNavigateToRoute: () -> Unit,
    viewModel: HomeViewModel = hiltViewModel()
) {
    val state by viewModel.uiState.collectAsState()

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(Canvas)
            .verticalScroll(rememberScrollState())
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp)
    ) {
        // ── Online / Offline toggle ───────────────────────────────────────────
        val statusColor = if (state.isOnline) Green else Color.White.copy(alpha = 0.4f)
        val statusLabel = if (state.isOnline) "ONLINE" else "OFFLINE"
        Card(
            modifier = Modifier.fillMaxWidth(),
            shape = RoundedCornerShape(16.dp),
            colors = CardDefaults.cardColors(
                containerColor = if (state.isOnline) Green.copy(alpha = 0.10f) else Glass
            ),
            border = androidx.compose.foundation.BorderStroke(
                1.dp,
                if (state.isOnline) Green.copy(alpha = 0.5f) else Border
            )
        ) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 20.dp, vertical = 16.dp),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically
            ) {
                Column {
                    Text(
                        text = statusLabel,
                        color = statusColor,
                        fontSize = 18.sp,
                        fontWeight = FontWeight.Bold,
                        letterSpacing = 2.sp
                    )
                    Text(
                        text = if (state.isOnline) "Accepting jobs" else "Not accepting jobs",
                        color = Color.White.copy(alpha = 0.5f),
                        fontSize = 12.sp
                    )
                }
                if (state.isTogglingStatus) {
                    CircularProgressIndicator(
                        color = if (state.isOnline) Green else Cyan,
                        modifier = Modifier.size(28.dp),
                        strokeWidth = 2.dp
                    )
                } else {
                    Switch(
                        checked = state.isOnline,
                        onCheckedChange = { viewModel.toggleOnlineStatus() },
                        colors = SwitchDefaults.colors(
                            checkedThumbColor = Canvas,
                            checkedTrackColor = Green,
                            uncheckedThumbColor = Color.White.copy(alpha = 0.6f),
                            uncheckedTrackColor = Color.White.copy(alpha = 0.15f)
                        )
                    )
                }
            }
        }

        if (state.isOfflineMode) {
            Card(
                modifier = Modifier.fillMaxWidth(),
                colors = CardDefaults.cardColors(containerColor = Amber.copy(alpha = 0.15f)),
                border = androidx.compose.foundation.BorderStroke(1.dp, Amber.copy(alpha = 0.4f))
            ) {
                Row(
                    modifier = Modifier.padding(12.dp),
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Text("\u26a0", fontSize = 16.sp)
                    Text(
                        "Offline Mode Active — reconnect to sync",
                        color = Amber, fontSize = 13.sp, fontWeight = FontWeight.Medium
                    )
                }
            }
        }

        state.error?.let { err ->
            Card(
                modifier = Modifier.fillMaxWidth(),
                colors = CardDefaults.cardColors(containerColor = Color(0xFFFF3B5C).copy(alpha = 0.12f)),
                border = androidx.compose.foundation.BorderStroke(1.dp, Color(0xFFFF3B5C).copy(alpha = 0.4f))
            ) {
                Text(
                    text = err,
                    color = Color(0xFFFF3B5C),
                    fontSize = 12.sp,
                    modifier = Modifier.padding(12.dp)
                )
            }
        }

        val shift = state.shift
        Card(
            modifier = Modifier.fillMaxWidth(),
            colors = CardDefaults.cardColors(containerColor = Glass),
            border = androidx.compose.foundation.BorderStroke(1.dp, Border)
        ) {
            Column(modifier = Modifier.padding(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Text("Today's Shift", color = Color.White.copy(alpha = 0.6f), fontSize = 13.sp)
                if (shift != null) {
                    Row(horizontalArrangement = Arrangement.spacedBy(24.dp)) {
                        StatItem(label = "Total", value = shift.totalStops.toString(), color = Color.White)
                        StatItem(label = "Done", value = shift.completedStops.toString(), color = Green)
                        StatItem(label = "Failed", value = shift.failedStops.toString(), color = Color(0xFFFF3B5C))
                        StatItem(label = "COD", value = "\u20b1${shift.totalCodCollected.toInt()}", color = Cyan)
                    }
                } else if (state.isLoading) {
                    CircularProgressIndicator(color = Cyan, modifier = Modifier.size(24.dp))
                } else {
                    Text("No active shift", color = Color.White.copy(alpha = 0.4f), fontSize = 14.sp)
                }
            }
        }

        Button(
            onClick = onNavigateToRoute,
            enabled = shift != null,
            modifier = Modifier
                .fillMaxWidth()
                .height(52.dp),
            colors = ButtonDefaults.buttonColors(containerColor = Cyan)
        ) {
            Text("View Route", color = Canvas, fontWeight = FontWeight.Bold, fontSize = 16.sp)
        }
    }
}

@Composable
private fun StatItem(label: String, value: String, color: Color) {
    Column(horizontalAlignment = Alignment.CenterHorizontally) {
        Text(value, color = color, fontSize = 22.sp, fontWeight = FontWeight.Bold)
        Text(label, color = Color.White.copy(alpha = 0.5f), fontSize = 11.sp)
    }
}
