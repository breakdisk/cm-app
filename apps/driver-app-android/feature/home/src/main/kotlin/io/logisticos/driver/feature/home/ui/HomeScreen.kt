package io.logisticos.driver.feature.home.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
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

val Canvas = Color(0xFF050810)
val Cyan = Color(0xFF00E5FF)
val Amber = Color(0xFFFFAB00)
val Green = Color(0xFF00FF88)
val Glass = Color(0x0AFFFFFF)
val Border = Color(0x14FFFFFF)

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
