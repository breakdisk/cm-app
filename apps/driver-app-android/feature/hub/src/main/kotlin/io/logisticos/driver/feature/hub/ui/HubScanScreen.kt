package io.logisticos.driver.feature.hub.ui

import android.Manifest
import android.content.pm.PackageManager
import androidx.camera.core.CameraSelector
import androidx.camera.core.ImageAnalysis
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
import androidx.compose.material.icons.filled.ArrowBack
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.QrCodeScanner
import androidx.compose.material.icons.filled.Warning
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
import io.logisticos.driver.feature.hub.domain.HubScanType
import io.logisticos.driver.feature.hub.presentation.HubScanViewModel
import io.logisticos.driver.feature.scanner.domain.ScanResult

// ── Colour tokens (match global design system) ────────────────────────────────
private val Canvas  = Color(0xFF050810)
private val Surface = Color(0xFF0A0E1A)
private val Cyan    = Color(0xFF00E5FF)
private val Purple  = Color(0xFFA855F7)
private val Green   = Color(0xFF00FF88)
private val Amber   = Color(0xFFFFAB00)
private val Red     = Color(0xFFFF3B5C)
private val Border  = Color(0x14FFFFFF)
private val TextMuted = Color(0x66FFFFFF)

/**
 * Hub Mode scan screen.
 *
 * Provides:
 * - Scan-type selector (INBOUND_RECEIVE → LOCAL_SORT_ASSIGN)
 * - Context fields (hub ID, master AWB, shipment ID, pallet/container IDs)
 * - Live camera barcode scanner (piece AWB) or manual entry
 * - Submit button with inline success/queued/error feedback
 *
 * @param initialHubId  Pre-filled from auth claims or hub session config.
 * @param initialScanType  Start on a specific scan type (e.g. INBOUND_RECEIVE on shift start).
 * @param onBack  Navigate up.
 */
@Composable
fun HubScanScreen(
    initialHubId:   String = "",
    initialScanType: HubScanType = HubScanType.INBOUND_RECEIVE,
    onBack:         () -> Unit,
    viewModel: HubScanViewModel = hiltViewModel(),
) {
    val state  by viewModel.uiState.collectAsState()
    val context       = LocalContext.current
    val lifecycleOwner = LocalLifecycleOwner.current

    // Pre-fill hub ID and scan type from caller on first composition.
    LaunchedEffect(Unit) {
        if (initialHubId.isNotBlank()) viewModel.setHubId(initialHubId)
        viewModel.setScanType(initialScanType)
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(Canvas)
            .verticalScroll(rememberScrollState())
            .padding(bottom = 32.dp),
    ) {
        // ── Header ──────────────────────────────────────────────────────────
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 16.dp, vertical = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            IconButton(onClick = onBack) {
                Icon(Icons.Default.ArrowBack, contentDescription = "Back", tint = Color.White.copy(alpha = 0.6f))
            }
            Column(modifier = Modifier.weight(1f)) {
                Text("Hub Mode", color = Color.White, fontWeight = FontWeight.Bold, fontSize = 18.sp)
                Text(state.scanType.label, color = Cyan, fontFamily = FontFamily.Monospace, fontSize = 12.sp)
            }
            Icon(Icons.Default.QrCodeScanner, contentDescription = null, tint = Cyan, modifier = Modifier.size(24.dp))
        }

        // ── Scan Type Selector ───────────────────────────────────────────────
        ScanTypeSelector(
            selected  = state.scanType,
            onSelect  = viewModel::setScanType,
            modifier  = Modifier.padding(horizontal = 16.dp),
        )

        if (state.scanType == HubScanType.EXCEPTION_FLAG) {
            ExceptionSubTypeSelector(
                selected = state.exception,
                onSelect = viewModel::setException,
                modifier = Modifier.padding(horizontal = 16.dp),
            )
        }

        Spacer(Modifier.height(12.dp))

        // ── Camera Viewfinder ────────────────────────────────────────────────
        CameraSection(
            onScanResult = { result ->
                // System.currentTimeMillis() captured at scan result callback — closest
                // available proxy to the hardware shutter moment on soft-camera devices.
                viewModel.onPieceScan(result, System.currentTimeMillis())
            },
            scannedAwb = state.pieceAwb,
            modifier   = Modifier
                .fillMaxWidth()
                .height(220.dp)
                .padding(horizontal = 16.dp)
                .clip(RoundedCornerShape(16.dp)),
        )

        Spacer(Modifier.height(12.dp))

        // ── Context Fields ───────────────────────────────────────────────────
        Column(modifier = Modifier.padding(horizontal = 16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            HubField(label = "Hub ID *", value = state.hubId, onValueChange = viewModel::setHubId,
                placeholder = "UUID of this hub")
            HubField(label = "Master AWB *", value = state.masterAwb, onValueChange = viewModel::setMasterAwb,
                placeholder = "e.g. CM-PHL-S0012345")
            HubField(label = "Shipment ID *", value = state.shipmentId, onValueChange = viewModel::setShipmentId,
                placeholder = "UUID from master AWB lookup")
            HubField(label = "Piece AWB (scanned)", value = state.pieceAwb, onValueChange = {
                viewModel.onPieceScan(ScanResult(it, "manual"), System.currentTimeMillis())
            }, placeholder = "Scan or type child AWB")
            if (state.scanType.requiresPallet) {
                HubField(label = "Pallet ID *", value = state.palletId, onValueChange = viewModel::setPalletId,
                    placeholder = "UUID of the target pallet")
            }
            if (state.scanType.requiresContainer) {
                HubField(label = "Container ID *", value = state.containerId, onValueChange = viewModel::setContainerId,
                    placeholder = "UUID of the container / vehicle")
            }
        }

        Spacer(Modifier.height(16.dp))

        // ── Submit ───────────────────────────────────────────────────────────
        Button(
            onClick = { viewModel.submitScan() },
            enabled = state.canSubmit,
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 16.dp)
                .height(52.dp),
            shape  = RoundedCornerShape(14.dp),
            colors = ButtonDefaults.buttonColors(
                containerColor         = Cyan.copy(alpha = 0.15f),
                contentColor           = Cyan,
                disabledContainerColor = Color.White.copy(alpha = 0.05f),
                disabledContentColor   = Color.White.copy(alpha = 0.25f),
            ),
        ) {
            if (state.isSubmitting) {
                CircularProgressIndicator(modifier = Modifier.size(18.dp), color = Cyan, strokeWidth = 2.dp)
            } else {
                Text("Submit Scan", fontWeight = FontWeight.Bold, fontSize = 15.sp)
            }
        }

        // ── Feedback ─────────────────────────────────────────────────────────
        Spacer(Modifier.height(8.dp))
        AnimatedVisibility(
            visible = state.lastSubmitSuccess == true,
            enter = fadeIn(), exit = fadeOut(),
        ) {
            FeedbackRow(
                icon  = Icons.Default.CheckCircle,
                color = Green,
                text  = if (state.lastSubmitQueued) "Queued — will sync when online" else "Scan recorded",
            )
        }
        AnimatedVisibility(visible = state.error != null, enter = fadeIn(), exit = fadeOut()) {
            FeedbackRow(icon = Icons.Default.Warning, color = Red, text = state.error ?: "")
        }

        // ── Scan type description hint ────────────────────────────────────────
        Text(
            text = state.scanType.description,
            color = TextMuted,
            fontFamily = FontFamily.Monospace,
            fontSize = 11.sp,
            modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp),
        )
    }
}

// ── Scan type chip row ────────────────────────────────────────────────────────

@Composable
private fun ScanTypeSelector(
    selected: HubScanType,
    onSelect: (HubScanType) -> Unit,
    modifier: Modifier = Modifier,
) {
    Row(
        modifier          = modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        HubScanType.entries.forEach { type ->
            val isSelected = type == selected
            Text(
                text     = type.label,
                color    = if (isSelected) Cyan else TextMuted,
                fontSize = 11.sp,
                fontFamily = FontFamily.Monospace,
                fontWeight = if (isSelected) FontWeight.Bold else FontWeight.Normal,
                modifier = Modifier
                    .border(
                        width = 1.dp,
                        color = if (isSelected) Cyan.copy(alpha = 0.5f) else Border,
                        shape = RoundedCornerShape(8.dp),
                    )
                    .background(
                        color = if (isSelected) Cyan.copy(alpha = 0.08f) else Color.Transparent,
                        shape = RoundedCornerShape(8.dp),
                    )
                    .clickable { onSelect(type) }
                    .padding(horizontal = 8.dp, vertical = 4.dp),
            )
        }
    }
}

// ── Exception sub-type selector (shown only for EXCEPTION_FLAG) ───────────────

private val exceptionOptions = listOf(
    "missing"         to "Missing",
    "damaged"         to "Damaged",
    "weight_mismatch" to "Weight Mismatch",
)

@Composable
private fun ExceptionSubTypeSelector(
    selected: String?,
    onSelect: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(modifier = modifier) {
        Text(
            "Exception type *",
            color      = Amber,
            fontFamily = FontFamily.Monospace,
            fontSize   = 11.sp,
            modifier   = Modifier.padding(bottom = 4.dp),
        )
        Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            exceptionOptions.forEach { (value, label) ->
                val isSelected = value == selected
                Text(
                    text       = label,
                    color      = if (isSelected) Amber else TextMuted,
                    fontSize   = 11.sp,
                    fontFamily = FontFamily.Monospace,
                    fontWeight = if (isSelected) FontWeight.Bold else FontWeight.Normal,
                    modifier   = Modifier
                        .border(
                            width = 1.dp,
                            color = if (isSelected) Amber.copy(alpha = 0.5f) else Border,
                            shape = RoundedCornerShape(8.dp),
                        )
                        .background(
                            color = if (isSelected) Amber.copy(alpha = 0.08f) else Color.Transparent,
                            shape = RoundedCornerShape(8.dp),
                        )
                        .clickable { onSelect(value) }
                        .padding(horizontal = 8.dp, vertical = 4.dp),
                )
            }
        }
    }
}

// ── Camera section ────────────────────────────────────────────────────────────

@Composable
private fun CameraSection(
    onScanResult: (ScanResult) -> Unit,
    scannedAwb:   String,
    modifier:     Modifier = Modifier,
) {
    val context       = LocalContext.current
    val lifecycleOwner = LocalLifecycleOwner.current
    val hasCameraPermission = remember(context) {
        ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) ==
                PackageManager.PERMISSION_GRANTED
    }

    Box(modifier = modifier.background(Surface)) {
        if (hasCameraPermission) {
            AndroidView(
                modifier = Modifier.fillMaxSize(),
                factory  = { ctx ->
                    val previewView = PreviewView(ctx)
                    val executor    = ContextCompat.getMainExecutor(ctx)
                    ProcessCameraProvider.getInstance(ctx).addListener({
                        val provider  = ProcessCameraProvider.getInstance(ctx).get()
                        val preview   = androidx.camera.core.Preview.Builder().build()
                            .also { it.setSurfaceProvider(previewView.surfaceProvider) }
                        val analyzer  = ImageAnalysis.Builder()
                            .setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST)
                            .build()
                        val scanner   = BarcodeScanning.getClient()
                        analyzer.setAnalyzer(executor) { imageProxy ->
                            val media = imageProxy.image ?: run { imageProxy.close(); return@setAnalyzer }
                            val image = InputImage.fromMediaImage(media, imageProxy.imageInfo.rotationDegrees)
                            scanner.process(image)
                                .addOnSuccessListener { codes ->
                                    codes.firstOrNull()?.rawValue?.let { raw ->
                                        onScanResult(ScanResult(raw, "qr"))
                                    }
                                }
                                .addOnCompleteListener { imageProxy.close() }
                        }
                        try {
                            provider.unbindAll()
                            provider.bindToLifecycle(lifecycleOwner, CameraSelector.DEFAULT_BACK_CAMERA, preview, analyzer)
                        } catch (e: Exception) {
                            android.util.Log.e("HubScan", "Camera bind failed: ${e.message}")
                        }
                    }, executor)
                    previewView
                }
            )
        } else {
            Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                Text("Camera permission required", color = TextMuted, fontFamily = FontFamily.Monospace, fontSize = 12.sp)
            }
        }

        // Scanned AWB overlay
        if (scannedAwb.isNotBlank()) {
            Box(
                modifier = Modifier
                    .align(Alignment.BottomStart)
                    .padding(8.dp)
                    .background(Surface.copy(alpha = 0.85f), RoundedCornerShape(8.dp))
                    .padding(horizontal = 10.dp, vertical = 4.dp),
            ) {
                Text(scannedAwb, color = Cyan, fontFamily = FontFamily.Monospace, fontSize = 12.sp)
            }
        }
    }
}

// ── Small helpers ─────────────────────────────────────────────────────────────

@Composable
private fun HubField(
    label:       String,
    value:       String,
    onValueChange: (String) -> Unit,
    placeholder: String = "",
) {
    Column {
        Text(label, color = TextMuted, fontFamily = FontFamily.Monospace, fontSize = 11.sp,
            modifier = Modifier.padding(bottom = 4.dp))
        OutlinedTextField(
            value         = value,
            onValueChange = onValueChange,
            placeholder   = { Text(placeholder, color = TextMuted, fontSize = 12.sp, fontFamily = FontFamily.Monospace) },
            singleLine    = true,
            textStyle     = LocalTextStyle.current.copy(color = Color.White, fontFamily = FontFamily.Monospace, fontSize = 13.sp),
            colors        = OutlinedTextFieldDefaults.colors(
                unfocusedBorderColor = Border,
                focusedBorderColor   = Cyan.copy(alpha = 0.6f),
                unfocusedContainerColor = Surface,
                focusedContainerColor   = Surface,
            ),
            shape = RoundedCornerShape(12.dp),
            modifier = Modifier.fillMaxWidth().height(50.dp),
        )
    }
}

@Composable
private fun FeedbackRow(icon: androidx.compose.ui.graphics.vector.ImageVector, color: Color, text: String) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp, vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Icon(icon, contentDescription = null, tint = color, modifier = Modifier.size(16.dp))
        Text(text, color = color, fontFamily = FontFamily.Monospace, fontSize = 12.sp)
    }
}
