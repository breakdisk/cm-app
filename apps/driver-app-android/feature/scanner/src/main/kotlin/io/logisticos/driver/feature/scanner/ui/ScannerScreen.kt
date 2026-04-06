package io.logisticos.driver.feature.scanner.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.hilt.navigation.compose.hiltViewModel
import io.logisticos.driver.feature.scanner.domain.ScanValidationResult
import io.logisticos.driver.feature.scanner.presentation.ScannerViewModel

private val Canvas = Color(0xFF050810)
private val Cyan = Color(0xFF00E5FF)
private val Green = Color(0xFF00FF88)
private val Amber = Color(0xFFFFAB00)
private val Glass = Color(0x0AFFFFFF)
private val Border = Color(0x14FFFFFF)

@Composable
fun ScannerScreen(
    expectedAwbs: List<String>,
    onAllScanned: () -> Unit,
    viewModel: ScannerViewModel = hiltViewModel()
) {
    val state by viewModel.uiState.collectAsState()

    LaunchedEffect(Unit) { viewModel.setExpectedAwbs(expectedAwbs) }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(Canvas)
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp)
    ) {
        Text(
            "${state.scannedAwbs.size} / ${state.expectedAwbs.size} scanned",
            color = Cyan,
            fontSize = 22.sp,
            fontWeight = FontWeight.Bold
        )

        LinearProgressIndicator(
            progress = {
                if (state.expectedAwbs.isEmpty()) 0f
                else state.scannedAwbs.size.toFloat() / state.expectedAwbs.size
            },
            modifier = Modifier.fillMaxWidth(),
            color = Cyan,
            trackColor = Glass
        )

        when (val v = state.lastValidation) {
            is ScanValidationResult.Match -> FeedbackCard("✓ ${v.awb}", Green, "Scanned")
            is ScanValidationResult.Unexpected -> {
                FeedbackCard("⚠ ${v.awb}", Amber, "Unexpected package")
                Button(
                    onClick = viewModel::acknowledgeUnexpected,
                    colors = ButtonDefaults.buttonColors(containerColor = Amber.copy(alpha = 0.2f))
                ) {
                    Text("Acknowledge & Continue", color = Amber)
                }
            }
            is ScanValidationResult.Duplicate -> FeedbackCard(
                "↩ ${v.awb}",
                Color.White.copy(alpha = 0.4f),
                "Already scanned"
            )
            null -> {}
        }

        LazyColumn(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            items(state.expectedAwbs) { awb ->
                val isScanned = awb in state.scannedAwbs
                Card(
                    colors = CardDefaults.cardColors(containerColor = Glass),
                    border = BorderStroke(
                        1.dp,
                        if (isScanned) Green.copy(alpha = 0.4f) else Border
                    )
                ) {
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(12.dp),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Text(
                            awb,
                            color = Color.White,
                            fontSize = 14.sp,
                            fontFamily = FontFamily.Monospace
                        )
                        Text(
                            if (isScanned) "✓" else "·",
                            color = if (isScanned) Green else Color.White.copy(alpha = 0.3f),
                            fontSize = 18.sp
                        )
                    }
                }
            }
        }

        if (state.allScanned) {
            Button(
                onClick = onAllScanned,
                modifier = Modifier
                    .fillMaxWidth()
                    .height(52.dp),
                colors = ButtonDefaults.buttonColors(containerColor = Green)
            ) {
                Text("All Scanned — Continue", color = Canvas, fontWeight = FontWeight.Bold)
            }
        }
    }
}

@Composable
private fun FeedbackCard(awb: String, color: Color, label: String) {
    Card(
        colors = CardDefaults.cardColors(containerColor = color.copy(alpha = 0.1f)),
        border = BorderStroke(1.dp, color.copy(alpha = 0.3f)),
        modifier = Modifier.fillMaxWidth()
    ) {
        Row(
            modifier = Modifier.padding(12.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp)
        ) {
            Text(awb, color = color, fontSize = 14.sp, fontWeight = FontWeight.Medium)
            Text(label, color = color.copy(alpha = 0.7f), fontSize = 12.sp)
        }
    }
}
