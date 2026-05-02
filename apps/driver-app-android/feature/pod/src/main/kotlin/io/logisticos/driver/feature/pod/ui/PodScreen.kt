package io.logisticos.driver.feature.pod.ui

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.graphics.Bitmap
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.camera.core.CameraSelector
import androidx.camera.core.ImageCapture
import androidx.camera.core.ImageCaptureException
import androidx.camera.core.Preview
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.camera.view.PreviewView
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.CameraAlt
import androidx.compose.material.icons.filled.Check
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalLifecycleOwner
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.content.ContextCompat
import androidx.hilt.navigation.compose.hiltViewModel
import io.logisticos.driver.feature.pod.presentation.FailureReason
import io.logisticos.driver.feature.pod.presentation.PodViewModel
import java.io.File
import java.io.FileOutputStream
import java.util.concurrent.Executors

private val Canvas = Color(0xFF050810)
private val Cyan   = Color(0xFF00E5FF)
private val Green  = Color(0xFF00FF88)
private val Amber  = Color(0xFFFFAB00)
private val Red    = Color(0xFFFF3B5C)
private val Glass  = Color(0x0AFFFFFF)
private val Border = Color(0x14FFFFFF)

@Composable
fun PodScreen(
    taskId: String,
    shipmentId: String = "",
    recipientName: String = "",
    requiresPhoto: Boolean,
    requiresSignature: Boolean,
    requiresOtp: Boolean,
    isCod: Boolean = false,
    codAmount: Double = 0.0,
    onCompleted: () -> Unit,
    onFailed: (reason: String) -> Unit = {},
    onBack: () -> Unit = {},
    viewModel: PodViewModel = hiltViewModel()
) {
    val state by viewModel.uiState.collectAsState()
    val context = LocalContext.current

    LaunchedEffect(taskId) {
        viewModel.setRequirements(
            taskId = taskId,
            shipmentId = shipmentId,
            recipientName = recipientName,
            requiresPhoto = requiresPhoto,
            requiresSignature = requiresSignature,
            requiresOtp = requiresOtp,
            isCod = isCod,
            codAmount = codAmount
        )
        // If shipmentId wasn't passed (e.g. navigated without it), load from local DB
        if (shipmentId.isBlank()) {
            viewModel.loadTaskMeta(taskId)
        }
    }

    // Success state
    if (state.isSubmitted) {
        Box(
            modifier = Modifier.fillMaxSize().background(Canvas),
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
                Text("POD Submitted", color = Green, fontSize = 24.sp, fontWeight = FontWeight.Bold)
                if (isCod && state.codCollected) {
                    Text(
                        "COD ₱${"%,.2f".format(codAmount)} collected",
                        color = Amber,
                        fontSize = 14.sp,
                        fontWeight = FontWeight.SemiBold
                    )
                }
                Button(
                    onClick = onCompleted,
                    modifier = Modifier.width(200.dp).height(48.dp),
                    colors = ButtonDefaults.buttonColors(containerColor = Green),
                    shape = RoundedCornerShape(12.dp)
                ) {
                    Text("Continue", color = Canvas, fontWeight = FontWeight.Bold)
                }
            }
        }
        return
    }

    // Failure reason sheet
    if (state.showFailureSheet) {
        FailureReasonSheet(
            onSelect = { reason ->
                viewModel.submitFailure(taskId, reason) { onFailed(reason.name) }
            },
            onDismiss = { viewModel.dismissFailureSheet() }
        )
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
            modifier = Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 16.dp),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically
        ) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                IconButton(
                    onClick = onBack,
                    modifier = Modifier.size(36.dp),
                    colors = IconButtonDefaults.iconButtonColors(contentColor = Color.White.copy(alpha = 0.7f)),
                ) {
                    Icon(
                        imageVector = Icons.AutoMirrored.Filled.ArrowBack,
                        contentDescription = "Back",
                        modifier = Modifier.size(20.dp),
                    )
                }
                Column {
                    Text("PROOF OF DELIVERY", color = Cyan, fontSize = 11.sp, fontWeight = FontWeight.Bold, letterSpacing = 1.sp)
                    Text("Capture evidence", color = Color.White, fontSize = 18.sp, fontWeight = FontWeight.Bold)
                }
            }
            // Step indicators
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                if (requiresPhoto)     StepDot("P", state.photoPath != null)
                if (requiresSignature) StepDot("S", state.signaturePath != null)
                if (requiresOtp)       StepDot("O", state.otpToken != null)
                if (isCod)             StepDot("₱", state.codCollected)
            }
        }

        // COD section
        if (isCod) {
            CodSection(
                amount = codAmount,
                collected = state.codCollected,
                onToggle = viewModel::onCodToggled
            )
            Spacer(Modifier.height(12.dp))
        }

        // Photo capture
        if (requiresPhoto) {
            PhotoSection(
                captured = state.photoPath != null,
                onCaptured = { path ->
                    viewModel.onPhotoCaptured(path)
                },
                taskId = taskId,
                context = context
            )
            Spacer(Modifier.height(12.dp))
        }

        // Signature
        if (requiresSignature) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 20.dp)
                    .clip(RoundedCornerShape(14.dp))
                    .background(Glass)
                    .border(
                        1.dp,
                        if (state.signaturePath != null) Green.copy(alpha = 0.3f) else Border,
                        RoundedCornerShape(14.dp)
                    )
                    .padding(16.dp),
                verticalArrangement = Arrangement.spacedBy(10.dp)
            ) {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween
                ) {
                    Text("Signature", color = Color.White.copy(alpha = 0.6f), fontSize = 12.sp)
                    if (state.signaturePath != null) Text("Captured ✓", color = Green, fontSize = 11.sp)
                }
                SignatureCanvas(
                    onSigned = { bitmap ->
                        val path = saveBitmap(context, bitmap, "sig_$taskId.png")
                        viewModel.onSignatureSaved(path)
                    },
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(160.dp)
                        .clip(RoundedCornerShape(10.dp))
                )
            }
            Spacer(Modifier.height(12.dp))
        }

        // OTP
        if (requiresOtp) {
            OtpPodSection(
                otpToken = state.otpToken,
                onOtpEntered = viewModel::onOtpEntered
            )
            Spacer(Modifier.height(12.dp))
        }

        Spacer(Modifier.height(12.dp))

        // Surface submit-time failures so the user sees the real error instead
        // of the old silent-enqueue-to-sync-queue behaviour.
        state.error?.let { err ->
            Surface(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 20.dp),
                color = Red.copy(alpha = 0.12f),
                border = androidx.compose.foundation.BorderStroke(1.dp, Red.copy(alpha = 0.4f)),
                shape = RoundedCornerShape(12.dp)
            ) {
                Text(
                    text = err,
                    color = Red,
                    fontSize = 12.sp,
                    modifier = Modifier.padding(12.dp)
                )
            }
            Spacer(Modifier.height(8.dp))
        }

        // Submit POD
        Button(
            onClick = { viewModel.submit(taskId) },
            enabled = state.canSubmit && !state.isSubmitting,
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 20.dp)
                .height(56.dp),
            shape = RoundedCornerShape(14.dp),
            colors = ButtonDefaults.buttonColors(
                containerColor = Cyan,
                disabledContainerColor = Color.White.copy(alpha = 0.08f)
            )
        ) {
            if (state.isSubmitting) {
                CircularProgressIndicator(color = Canvas, modifier = Modifier.size(20.dp), strokeWidth = 2.dp)
            } else {
                Text(
                    "Submit POD",
                    color = if (state.canSubmit) Canvas else Color.White.copy(alpha = 0.3f),
                    fontWeight = FontWeight.Bold,
                    fontSize = 16.sp
                )
            }
        }

        Spacer(Modifier.height(8.dp))

        // Failed delivery button
        TextButton(
            onClick = { viewModel.showFailureSheet() },
            modifier = Modifier.fillMaxWidth().padding(horizontal = 20.dp)
        ) {
            Text("Mark as Failed Delivery", color = Red.copy(alpha = 0.7f), fontSize = 14.sp)
        }

        Spacer(Modifier.navigationBarsPadding().height(16.dp))
    }
}

@Composable
private fun StepDot(label: String, done: Boolean) {
    Box(
        modifier = Modifier
            .size(28.dp)
            .clip(RoundedCornerShape(14.dp))
            .background(if (done) Green.copy(alpha = 0.15f) else Glass)
            .border(1.dp, if (done) Green.copy(alpha = 0.4f) else Border, RoundedCornerShape(14.dp)),
        contentAlignment = Alignment.Center
    ) {
        Text(label, color = if (done) Green else Color.White.copy(alpha = 0.4f), fontSize = 11.sp, fontWeight = FontWeight.Bold)
    }
}

@Composable
private fun CodSection(amount: Double, collected: Boolean, onToggle: (Boolean) -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 20.dp)
            .clip(RoundedCornerShape(14.dp))
            .background(if (collected) Amber.copy(alpha = 0.08f) else Glass)
            .border(
                1.dp,
                if (collected) Amber.copy(alpha = 0.3f) else Border,
                RoundedCornerShape(14.dp)
            )
            .padding(16.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically
    ) {
        Column {
            Text("Cash on Delivery", color = Amber, fontSize = 13.sp, fontWeight = FontWeight.SemiBold)
            Text("₱${"%,.2f".format(amount)}", color = Color.White, fontSize = 22.sp, fontWeight = FontWeight.Bold)
        }
        Switch(
            checked = collected,
            onCheckedChange = onToggle,
            colors = SwitchDefaults.colors(
                checkedThumbColor = Canvas,
                checkedTrackColor = Amber,
                uncheckedThumbColor = Color.White.copy(alpha = 0.4f),
                uncheckedTrackColor = Color.White.copy(alpha = 0.08f)
            )
        )
    }
}

@Composable
private fun PhotoSection(
    captured: Boolean,
    onCaptured: (String) -> Unit,
    taskId: String,
    context: Context
) {
    val lifecycleOwner = LocalLifecycleOwner.current
    var showCamera by remember { mutableStateOf(false) }
    val imageCapture = remember { ImageCapture.Builder().build() }

    // Runtime CAMERA permission gate. Manifest declares CAMERA but Android
    // 6+ requires a runtime grant. Without this, ProcessCameraProvider's
    // bindToLifecycle below throws SecurityException on the first photo
    // attempt and crashes the activity. Pattern matches how HomeScreen
    // gates ACCESS_FINE_LOCATION before binding the location provider.
    var cameraPermissionGranted by remember {
        mutableStateOf(
            ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA)
                == PackageManager.PERMISSION_GRANTED
        )
    }
    val cameraPermissionLauncher = rememberLauncherForActivityResult(
        contract = ActivityResultContracts.RequestPermission()
    ) { granted ->
        cameraPermissionGranted = granted
        if (granted) showCamera = true
    }

    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 20.dp)
            .clip(RoundedCornerShape(14.dp))
            .background(Glass)
            .border(
                1.dp,
                if (captured) Green.copy(alpha = 0.3f) else Border,
                RoundedCornerShape(14.dp)
            )
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp)
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween
        ) {
            Text("Parcel Photo", color = Color.White.copy(alpha = 0.6f), fontSize = 12.sp)
            if (captured) Text("Captured ✓", color = Green, fontSize = 11.sp)
        }

        if (showCamera) {
            var cameraError by remember { mutableStateOf<String?>(null) }

            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .height(240.dp)
                    .clip(RoundedCornerShape(10.dp))
            ) {
                if (cameraError != null) {
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
                            Text(cameraError ?: "", color = Color.White.copy(alpha = 0.5f), fontSize = 12.sp)
                            TextButton(onClick = { showCamera = false; cameraError = null }) {
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
                                    cameraProvider.unbindAll()
                                    cameraProvider.bindToLifecycle(
                                        lifecycleOwner,
                                        CameraSelector.DEFAULT_BACK_CAMERA,
                                        preview,
                                        imageCapture
                                    )
                                } catch (e: Exception) {
                                    android.util.Log.e("PodScreen", "Camera bind failed: ${e.message}", e)
                                    cameraError = e.message ?: "Camera failed to start"
                                }
                            }, ContextCompat.getMainExecutor(ctx))
                            previewView
                        },
                        modifier = Modifier.fillMaxSize()
                    )

                    Button(
                        onClick = {
                            val file = File(context.filesDir, "photo_$taskId.jpg")
                            val outputOptions = ImageCapture.OutputFileOptions.Builder(file).build()
                            imageCapture.takePicture(
                                outputOptions,
                                Executors.newSingleThreadExecutor(),
                                object : ImageCapture.OnImageSavedCallback {
                                    override fun onImageSaved(output: ImageCapture.OutputFileResults) {
                                        showCamera = false
                                        onCaptured(file.absolutePath)
                                    }
                                    override fun onError(exc: ImageCaptureException) {
                                        android.util.Log.e("PodScreen", "Photo capture failed: ${exc.message}", exc)
                                        cameraError = "Capture failed: ${exc.message}"
                                    }
                                }
                            )
                        },
                        modifier = Modifier.align(Alignment.BottomCenter).padding(16.dp).fillMaxWidth().height(48.dp),
                        colors = ButtonDefaults.buttonColors(containerColor = Cyan),
                        shape = RoundedCornerShape(12.dp)
                    ) {
                        Icon(Icons.Default.CameraAlt, contentDescription = null, tint = Canvas, modifier = Modifier.size(18.dp))
                        Spacer(Modifier.width(8.dp))
                        Text("Capture", color = Canvas, fontWeight = FontWeight.Bold)
                    }
                }
            }
        } else {
            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .height(120.dp)
                    .clip(RoundedCornerShape(10.dp))
                    .background(Color(0x08FFFFFF))
                    .border(1.dp, Border, RoundedCornerShape(10.dp)),
                contentAlignment = Alignment.Center
            ) {
                if (captured) {
                    Text("📷  Photo captured", color = Green, fontSize = 14.sp)
                } else {
                    Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        Text("📷", fontSize = 28.sp)
                        Text("Tap to open camera", color = Color.White.copy(alpha = 0.3f), fontSize = 12.sp)
                    }
                }
            }

            Button(
                onClick = {
                    if (cameraPermissionGranted) {
                        showCamera = true
                    } else {
                        // Triggers the OS permission dialog. The launcher's
                        // callback flips showCamera once the user accepts.
                        // If they deny, we stay on the placeholder card —
                        // no crash, no broken state.
                        cameraPermissionLauncher.launch(Manifest.permission.CAMERA)
                    }
                },
                modifier = Modifier.fillMaxWidth().height(44.dp),
                colors = ButtonDefaults.buttonColors(containerColor = Cyan.copy(alpha = 0.12f)),
                shape = RoundedCornerShape(10.dp)
            ) {
                Icon(Icons.Default.CameraAlt, contentDescription = null, tint = Cyan, modifier = Modifier.size(16.dp))
                Spacer(Modifier.width(8.dp))
                Text(if (captured) "Retake Photo" else "Open Camera", color = Cyan, fontSize = 14.sp)
            }
        }
    }
}

@Composable
private fun OtpPodSection(otpToken: String?, onOtpEntered: (String) -> Unit) {
    var entered by remember { mutableStateOf("") }
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 20.dp)
            .clip(RoundedCornerShape(14.dp))
            .background(Glass)
            .border(
                1.dp,
                if (otpToken != null) Green.copy(alpha = 0.3f) else Border,
                RoundedCornerShape(14.dp)
            )
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp)
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween
        ) {
            Text("OTP Verification", color = Color.White.copy(alpha = 0.6f), fontSize = 12.sp)
            if (otpToken != null) Text("Verified ✓", color = Green, fontSize = 11.sp)
        }
        Text("Ask recipient for their one-time delivery code", color = Color.White.copy(alpha = 0.5f), fontSize = 13.sp)
        OutlinedTextField(
            value = entered,
            onValueChange = { if (it.length <= 6) entered = it },
            label = { Text("6-digit OTP") },
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
            colors = OutlinedTextFieldDefaults.colors(
                focusedBorderColor = Cyan,
                unfocusedBorderColor = Border,
                focusedTextColor = Color.White,
                unfocusedTextColor = Color.White,
                focusedLabelColor = Cyan,
                unfocusedLabelColor = Color.White.copy(alpha = 0.5f),
                cursorColor = Cyan
            )
        )
        Button(
            onClick = { if (entered.length == 6) onOtpEntered(entered) },
            enabled = entered.length == 6 && otpToken == null,
            modifier = Modifier.fillMaxWidth().height(44.dp),
            colors = ButtonDefaults.buttonColors(containerColor = Cyan),
            shape = RoundedCornerShape(10.dp)
        ) {
            Text("Confirm OTP", color = Canvas)
        }
    }
}

@Composable
private fun FailureReasonSheet(
    onSelect: (FailureReason) -> Unit,
    onDismiss: () -> Unit
) {
    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(Color.Black.copy(alpha = 0.6f)),
        contentAlignment = Alignment.BottomCenter
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(topStart = 20.dp, topEnd = 20.dp))
                .background(Color(0xFF0D1220))
                .border(1.dp, Border, RoundedCornerShape(topStart = 20.dp, topEnd = 20.dp))
                .padding(horizontal = 20.dp, vertical = 24.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp)
        ) {
            Text("Failure Reason", color = Red, fontSize = 16.sp, fontWeight = FontWeight.Bold)
            Text("Select the reason this delivery could not be completed", color = Color.White.copy(alpha = 0.5f), fontSize = 13.sp)
            Spacer(Modifier.height(4.dp))

            FailureReason.entries.forEach { reason ->
                Button(
                    onClick = { onSelect(reason) },
                    modifier = Modifier.fillMaxWidth().height(48.dp),
                    shape = RoundedCornerShape(10.dp),
                    colors = ButtonDefaults.buttonColors(containerColor = Glass)
                ) {
                    Text(reason.displayName, color = Color.White, fontSize = 14.sp)
                }
            }

            Spacer(Modifier.height(8.dp))
            TextButton(onClick = onDismiss, modifier = Modifier.fillMaxWidth()) {
                Text("Cancel", color = Color.White.copy(alpha = 0.4f))
            }
            Spacer(Modifier.navigationBarsPadding())
        }
    }
}

private fun saveBitmap(context: Context, bitmap: Bitmap, filename: String): String {
    val file = File(context.filesDir, filename)
    FileOutputStream(file).use { out -> bitmap.compress(Bitmap.CompressFormat.PNG, 90, out) }
    return file.absolutePath
}
