package io.logisticos.driver.feature.pickup.ui

import android.Manifest
import android.content.pm.PackageManager
import android.graphics.Bitmap
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.camera.core.CameraSelector
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.Preview
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.camera.view.PreviewView
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.CameraAlt
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.QrCodeScanner
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalLifecycleOwner
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.content.ContextCompat
import androidx.hilt.navigation.compose.hiltViewModel
import com.google.mlkit.vision.barcode.BarcodeScanning
import com.google.mlkit.vision.common.InputImage
import io.logisticos.driver.feature.pickup.presentation.PickupViewModel
import java.io.File
import java.io.FileOutputStream

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
    onBack: () -> Unit = {},
    viewModel: PickupViewModel = hiltViewModel()
) {
    val state by viewModel.uiState.collectAsState()
    val context = LocalContext.current
    val lifecycleOwner = LocalLifecycleOwner.current

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
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                IconButton(
                    onClick = onBack,
                    modifier = Modifier.size(36.dp),
                    colors = IconButtonDefaults.iconButtonColors(
                        contentColor = Color.White.copy(alpha = 0.7f),
                    ),
                ) {
                    Icon(
                        imageVector = Icons.AutoMirrored.Filled.ArrowBack,
                        contentDescription = "Back",
                        modifier = Modifier.size(20.dp),
                    )
                }
                Column {
                    Text("FIRST MILE", color = Purple, fontSize = 11.sp, fontWeight = FontWeight.Bold, letterSpacing = 1.sp)
                    Text("Pickup Confirmation", color = Color.White, fontSize = 20.sp, fontWeight = FontWeight.Bold)
                }
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

        // ── AWB scan section ──────────────────────────────────────────────────
        // Camera permission state for QR scanning
        var showQrCamera by remember { mutableStateOf(false) }
        var qrCameraError by remember { mutableStateOf<String?>(null) }
        var cameraPermissionGranted by remember {
            mutableStateOf(
                ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA)
                    == PackageManager.PERMISSION_GRANTED
            )
        }
        val qrPermissionLauncher = rememberLauncherForActivityResult(
            ActivityResultContracts.RequestPermission()
        ) { granted ->
            cameraPermissionGranted = granted
            if (granted) showQrCamera = true
        }

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

            // Expected AWB
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

            // ── Inline QR camera preview ──────────────────────────────────
            if (showQrCamera) {
                Box(
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(220.dp)
                        .clip(RoundedCornerShape(10.dp))
                ) {
                    if (qrCameraError != null) {
                        Box(
                            modifier = Modifier
                                .fillMaxSize()
                                .background(Color(0xFF1A0A0A))
                                .border(1.dp, Red.copy(alpha = 0.4f), RoundedCornerShape(10.dp)),
                            contentAlignment = Alignment.Center
                        ) {
                            Column(
                                horizontalAlignment = Alignment.CenterHorizontally,
                                verticalArrangement = Arrangement.spacedBy(8.dp),
                                modifier = Modifier.padding(16.dp)
                            ) {
                                Text("Camera unavailable", color = Red, fontSize = 14.sp, fontWeight = FontWeight.SemiBold)
                                Text(qrCameraError ?: "", color = Color.White.copy(alpha = 0.5f), fontSize = 12.sp)
                                TextButton(onClick = { showQrCamera = false; qrCameraError = null }) {
                                    Text("Dismiss", color = Cyan)
                                }
                            }
                        }
                    } else {
                        AndroidView(
                            factory = { ctx ->
                                val previewView = PreviewView(ctx)
                                val cameraProviderFuture = ProcessCameraProvider.getInstance(ctx)
                                cameraProviderFuture.addListener({
                                    try {
                                        val cameraProvider = cameraProviderFuture.get()
                                        val preview = Preview.Builder().build().also {
                                            it.setSurfaceProvider(previewView.surfaceProvider)
                                        }
                                        val barcodeScanner = BarcodeScanning.getClient()
                                        val imageAnalysis = ImageAnalysis.Builder()
                                            .setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST)
                                            .build()
                                        imageAnalysis.setAnalyzer(
                                            ContextCompat.getMainExecutor(ctx)
                                        ) { imageProxy ->
                                            @Suppress("UnsafeOptInUsageError")
                                            val mediaImage = imageProxy.image
                                            if (mediaImage != null) {
                                                val image = InputImage.fromMediaImage(
                                                    mediaImage,
                                                    imageProxy.imageInfo.rotationDegrees
                                                )
                                                barcodeScanner.process(image)
                                                    .addOnSuccessListener { barcodes ->
                                                        barcodes.firstOrNull()?.rawValue?.let { value ->
                                                            viewModel.onAwbScanned(value)
                                                            showQrCamera = false
                                                        }
                                                    }
                                                    .addOnCompleteListener { imageProxy.close() }
                                            } else {
                                                imageProxy.close()
                                            }
                                        }
                                        cameraProvider.unbindAll()
                                        cameraProvider.bindToLifecycle(
                                            lifecycleOwner,
                                            CameraSelector.DEFAULT_BACK_CAMERA,
                                            preview,
                                            imageAnalysis
                                        )
                                    } catch (e: Exception) {
                                        android.util.Log.e("PickupScreen", "QR camera bind failed: ${e.message}", e)
                                        qrCameraError = e.message ?: "Camera failed to start"
                                    }
                                }, ContextCompat.getMainExecutor(ctx))
                                previewView
                            },
                            modifier = Modifier.fillMaxSize()
                        )
                        // Overlay: close scanner button
                        IconButton(
                            onClick = { showQrCamera = false },
                            modifier = Modifier
                                .align(Alignment.TopEnd)
                                .padding(6.dp)
                                .size(36.dp)
                                .clip(RoundedCornerShape(18.dp))
                                .background(Color.Black.copy(alpha = 0.5f))
                        ) {
                            Icon(Icons.Default.Close, contentDescription = "Close scanner", tint = Color.White, modifier = Modifier.size(18.dp))
                        }
                        // Overlay: scan hint
                        Box(
                            modifier = Modifier
                                .align(Alignment.BottomCenter)
                                .padding(bottom = 10.dp)
                                .clip(RoundedCornerShape(8.dp))
                                .background(Color.Black.copy(alpha = 0.5f))
                                .padding(horizontal = 12.dp, vertical = 6.dp)
                        ) {
                            Text("Point camera at barcode", color = Color.White, fontSize = 12.sp)
                        }
                    }
                }
            }

            // Scan / toggle button
            Button(
                onClick = {
                    if (showQrCamera) {
                        showQrCamera = false
                    } else if (cameraPermissionGranted) {
                        qrCameraError = null
                        showQrCamera = true
                    } else {
                        qrPermissionLauncher.launch(Manifest.permission.CAMERA)
                    }
                },
                modifier = Modifier.fillMaxWidth().height(44.dp),
                colors = ButtonDefaults.buttonColors(
                    containerColor = if (showQrCamera) Red.copy(alpha = 0.12f) else Cyan.copy(alpha = 0.12f)
                ),
                shape = RoundedCornerShape(10.dp)
            ) {
                Icon(
                    if (showQrCamera) Icons.Default.Close else Icons.Default.QrCodeScanner,
                    contentDescription = null,
                    tint = if (showQrCamera) Red else Cyan,
                    modifier = Modifier.size(16.dp)
                )
                Spacer(Modifier.width(8.dp))
                Text(
                    if (showQrCamera) "Close Scanner" else "Scan QR / Barcode",
                    color = if (showQrCamera) Red else Cyan,
                    fontSize = 14.sp
                )
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
                        Icon(Icons.Default.Check, contentDescription = "Submit", tint = Cyan)
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

        // Photo section — system camera launcher returns a thumbnail Bitmap.
        val cameraLauncher = rememberLauncherForActivityResult(
            contract = ActivityResultContracts.TakePicturePreview(),
        ) { bitmap: Bitmap? ->
            if (bitmap != null) {
                val file = File(context.filesDir, "pickup_${taskId}.jpg")
                FileOutputStream(file).use { out ->
                    bitmap.compress(Bitmap.CompressFormat.JPEG, 90, out)
                }
                viewModel.onPhotoCaptured(file.absolutePath)
            }
        }

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

            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .height(120.dp)
                    .clip(RoundedCornerShape(10.dp))
                    .background(Color(0x08FFFFFF))
                    .border(1.dp, Border, RoundedCornerShape(10.dp))
                    .clickable { cameraLauncher.launch(null) },
                contentAlignment = Alignment.Center
            ) {
                if (state.photoPath != null) {
                    Text("📷  Photo captured — tap to retake", color = Green, fontSize = 14.sp)
                } else {
                    Column(
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.spacedBy(4.dp)
                    ) {
                        Text("📷", fontSize = 28.sp)
                        Text("Tap to capture parcel photo", color = Color.White.copy(alpha = 0.3f), fontSize = 12.sp)
                    }
                }
            }

            Button(
                onClick = { cameraLauncher.launch(null) },
                modifier = Modifier.fillMaxWidth().height(44.dp),
                colors = ButtonDefaults.buttonColors(containerColor = Cyan.copy(alpha = 0.12f)),
                shape = RoundedCornerShape(10.dp),
            ) {
                Icon(
                    Icons.Default.CameraAlt,
                    contentDescription = null,
                    tint = Cyan,
                    modifier = Modifier.size(16.dp),
                )
                Spacer(Modifier.width(8.dp))
                Text(
                    if (state.photoPath != null) "Retake Photo" else "Open Camera",
                    color = Cyan,
                    fontSize = 14.sp,
                )
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
                "Scan the AWB barcode or enter it manually to enable confirmation",
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
