package io.logisticos.driver.feature.navigation.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.hilt.navigation.compose.hiltViewModel
import io.logisticos.driver.feature.navigation.presentation.NavigationViewModel

@Composable
fun NavigationScreen(
    taskId: String,
    onArrived: () -> Unit
) {
    val viewModel: NavigationViewModel = hiltViewModel(
        creationCallback = { factory: NavigationViewModel.Factory -> factory.create(taskId) }
    )
    val state by viewModel.uiState.collectAsState()

    Box(modifier = Modifier.fillMaxSize()) {
        MapboxMapView(
            modifier = Modifier.fillMaxSize(),
            driverLat = state.currentLat,
            driverLng = state.currentLng,
            driverBearing = state.currentBearing,
            polylineEncoded = state.route?.polylineEncoded,
            stopLat = state.task?.lat ?: 0.0,
            stopLng = state.task?.lng ?: 0.0
        )

        if (state.nextInstruction.isNotEmpty()) {
            Surface(
                modifier = Modifier
                    .fillMaxWidth()
                    .align(Alignment.TopCenter)
                    .padding(16.dp),
                color = Color(0xE6050810),
                shape = MaterialTheme.shapes.medium
            ) {
                Row(
                    modifier = Modifier.padding(16.dp),
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Text("↑", color = Color(0xFF00E5FF), fontSize = 24.sp)
                    Column {
                        Text(
                            state.nextInstruction,
                            color = Color.White,
                            fontSize = 16.sp,
                            fontWeight = FontWeight.Medium
                        )
                        Text(
                            "${state.distanceToNextM}m",
                            color = Color.White.copy(alpha = 0.6f),
                            fontSize = 13.sp
                        )
                    }
                }
            }
        }

        state.task?.let { task ->
            Surface(
                modifier = Modifier
                    .fillMaxWidth()
                    .align(Alignment.BottomCenter)
                    .padding(16.dp),
                color = Color(0xE60A0E1A),
                shape = MaterialTheme.shapes.large
            ) {
                Column(
                    modifier = Modifier.padding(20.dp),
                    verticalArrangement = Arrangement.spacedBy(8.dp)
                ) {
                    Text(
                        task.recipientName,
                        color = Color.White,
                        fontSize = 18.sp,
                        fontWeight = FontWeight.Bold
                    )
                    Text(
                        task.address,
                        color = Color.White.copy(alpha = 0.6f),
                        fontSize = 14.sp
                    )
                    state.route?.let { route ->
                        Text(
                            "${route.distanceMeters / 1000.0}km · ETA ${formatEta(route.etaTimestamp)}",
                            color = Color(0xFF00E5FF),
                            fontSize = 13.sp
                        )
                    }
                    val buttonText = if (state.isArrived) "Confirm Arrival" else "I've Arrived"
                    val buttonColor = if (state.isArrived) Color(0xFF00FF88) else Color(0xFF00E5FF)
                    Button(
                        onClick = onArrived,
                        modifier = Modifier
                            .fillMaxWidth()
                            .height(48.dp),
                        colors = ButtonDefaults.buttonColors(
                            containerColor = buttonColor
                        )
                    ) {
                        Text(
                            buttonText,
                            color = Color(0xFF050810),
                            fontWeight = FontWeight.Bold
                        )
                    }
                }
            }
        }
    }
}

private fun formatEta(timestamp: Long): String {
    val mins = ((timestamp - System.currentTimeMillis()) / 60_000).toInt().coerceAtLeast(0)
    return if (mins < 60) "$mins min" else "${mins / 60}h ${mins % 60}min"
}
