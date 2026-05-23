package io.logisticos.driver.feature.scanner.ui

import android.Manifest
import android.content.pm.PackageManager
import androidx.camera.core.CameraSelector
import androidx.camera.core.ImageAnalysis
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.camera.view.PreviewView
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
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
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalLifecycleOwner
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.content.ContextCompat
import com.google.mlkit.vision.barcode.BarcodeScanning
import com.google.mlkit.vision.common.InputImage
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
    val context = LocalContext.current
    val lifecycleOwner = LocalLifecycleOwner.current

    LaunchedEffect(Unit) { viewModel.setExpectedAwbs(expectedAwbs) }

    // Hardware scanners register a broadcast receiver; software devices bind the camera.
    LaunchedEffect(viewModel.isHardwareScanner) {
        if (viewModel.isHardwareScanner) viewModel.startHardwareScan()
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(Canvas)
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp)
    ) {
        // Software-device camera viewfinder — hidden on hardware scanners.
        // ProcessCameraProvider binds an ImageAnalysis use case to the lifecycle;
        // MLKit processes each frame and routes scan results through the ViewModel.
        if (!viewModel.isHardwareScanner) {
            var cameraPermissionGranted by remember {
                mutableStateOf(
                    ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA)
                        == PackageManager.PERMISSION_GRANTED
                )
            }
            if (cameraPermissionGranted) {
                Box(
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(180.dp)
                ) {
                    val barcodeScanner = remember { BarcodeScanning.getClient() }
                    DisposableEffect(lifecycleOwner) {
                        val cameraProviderFuture = ProcessCameraProvider.getInstance(context)
                        cameraProviderFuture.addListener({
                            val provider = cameraProviderFuture.get()
                            val imageAnalysis = ImageAnalysis.Builder()
                                .setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST)
                                .build()
                            imageAnalysis.setAnalyzer(ContextCompat.getMainExecutor(context)) { proxy ->
                                @Suppress("UnsafeOptInUsageError")
                                val mediaImage = proxy.image
                                if (mediaImage != null) {
                                    val image = InputImage.fromMediaImage(
                                        mediaImage, proxy.imageInfo.rotationDegrees
                                    )
                                    barcodeScanner.process(image)
                                        .addOnSuccessListener { barcodes ->
                                            barcodes.firstOrNull()?.rawValue?.let { value ->
                                                viewModel.onScanResult(
                                                    io.logisticos.driver.feature.scanner.domain.ScanResult(
                                                        rawValue = value,
                                                        format = barcodes.first().format.toString()
                                                    )
                                                )
                                            }
                                        }
                                        .addOnCompleteListener { proxy.close() }
                                } else {
                                    proxy.close()
                                }
                            }
                            provider.unbindAll()
                            provider.bindToLifecycle(
                                lifecycleOwner, CameraSelector.DEFAULT_BACK_CAMERA, imageAnalysis
                            )
                        }, ContextCompat.getMainExecutor(context))
                        onDispose {
                            runCatching { ProcessCameraProvider.getInstance(context).get().unbindAll() }
                            barcodeScanner.close()
                        }
                    }
                    AndroidView(
                        factory = { ctx ->
                            val previewView = PreviewView(ctx)
                            val cameraProviderFuture = ProcessCameraProvider.getInstance(ctx)
                            cameraProviderFuture.addListener({
                                val provider = cameraProviderFuture.get()
                                val preview = androidx.camera.core.Preview.Builder().build().also {
                                    it.setSurfaceProvider(previewView.surfaceProvider)
                                }
                                try {
                                    provider.bindToLifecycle(
                                        lifecycleOwner, CameraSelector.DEFAULT_BACK_CAMERA, preview
                                    )
                                } catch (_: Exception) {}
                            }, ContextCompat.getMainExecutor(ctx))
                            previewView
                        },
                        modifier = Modifier.fillMaxSize()
                    )
                    Box(
                        modifier = Modifier
                            .align(Alignment.BottomCenter)
                            .padding(bottom = 8.dp)
                            .background(Color.Black.copy(alpha = 0.5f))
                            .padding(horizontal = 12.dp, vertical = 4.dp)
                    ) {
                        Text("Point camera at barcode", color = Color.White, fontSize = 11.sp)
                    }
                }
            } else {
                // Camera permission not yet granted — request it.
                androidx.activity.compose.rememberLauncherForActivityResult(
                    androidx.activity.result.contract.ActivityResultContracts.RequestPermission()
                ) { granted -> cameraPermissionGranted = granted }
                    .also { launcher ->
                        LaunchedEffect(Unit) { launcher.launch(Manifest.permission.CAMERA) }
                    }
            }
        }
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
