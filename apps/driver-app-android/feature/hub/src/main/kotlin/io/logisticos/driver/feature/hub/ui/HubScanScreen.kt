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
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalLifecycleOwner
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.content.ContextCompat
import androidx.hilt.navigation.compose.hiltViewModel
import com.google.mlkit.vision.barcode.BarcodeScanning
import com.google.mlkit.vision.common.InputImage
import io.logisticos.driver.core.designsystem.*
import io.logisticos.driver.feature.hub.domain.HubScanType
import io.logisticos.driver.feature.hub.presentation.HubScanViewModel
import io.logisticos.driver.feature.scanner.domain.ScanResult

/**
 * Hub Mode scan screen (driver design, "hub").
 *
 * Provides:
 * - Scan-type selector (INBOUND_RECEIVE → LOCAL_SORT_ASSIGN), scrolling sideways
 * - Live camera barcode scanner in the bracketed viewfinder, or manual entry
 * - Context fields (hub ID, master AWB, shipment ID, pallet/container IDs)
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
    val state by viewModel.uiState.collectAsState()
    val c = LocalMoveColors.current

    // Pre-fill hub ID and scan type from caller on first composition.
    LaunchedEffect(Unit) {
        if (initialHubId.isNotBlank()) viewModel.setHubId(initialHubId)
        viewModel.setScanType(initialScanType)
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(c.ground)
            .verticalScroll(rememberScrollState())
            .padding(bottom = 32.dp),
    ) {
        MoveScreenHeader(label = "Hub mode · ${state.scanType.label}", title = "Scan at the hub", onBack = onBack)

        // ── Scan type ────────────────────────────────────────────────────────
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .horizontalScroll(rememberScrollState())
                .padding(horizontal = 16.dp),
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            HubScanType.entries.forEach { type ->
                MoveChip(label = type.label, selected = type == state.scanType, onClick = { viewModel.setScanType(type) })
            }
        }
        Text(
            text = state.scanType.description,
            color = c.muted,
            fontSize = 14.sp,
            modifier = Modifier.padding(horizontal = 16.dp, vertical = 10.dp),
        )

        if (state.scanType == HubScanType.EXCEPTION_FLAG) {
            Column(Modifier.padding(start = 16.dp, end = 16.dp, bottom = 12.dp)) {
                MoveLabel("Exception type *", dot = c.amber)
                Row(
                    modifier = Modifier.horizontalScroll(rememberScrollState()).padding(top = 10.dp),
                    horizontalArrangement = Arrangement.spacedBy(10.dp),
                ) {
                    exceptionOptions.forEach { (value, label) ->
                        MoveChip(label = label, selected = value == state.exception, onClick = { viewModel.setException(value) }, tone = MoveTone.Amber)
                    }
                }
            }
        }

        // ── Camera viewfinder ────────────────────────────────────────────────
        MoveViewfinder(Modifier.padding(horizontal = 16.dp).height(228.dp)) {
            CameraSection(
                onScanResult = { result ->
                    // System.currentTimeMillis() captured at scan result callback — closest
                    // available proxy to the hardware shutter moment on soft-camera devices.
                    viewModel.onPieceScan(result, System.currentTimeMillis())
                },
            )
        }

        if (state.pieceAwb.isNotBlank()) {
            MovePanel(Modifier.padding(start = 16.dp, end = 16.dp, top = 14.dp), tone = MoveTone.Accent) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Icon(Icons.Filled.CheckCircle, contentDescription = null, tint = c.accent, modifier = Modifier.size(20.dp))
                    Text("SCANNED", color = c.accent, fontSize = 12.sp, fontWeight = FontWeight.Bold, letterSpacing = 2.4.sp)
                }
                Text(
                    state.pieceAwb,
                    color = c.ink,
                    fontFamily = Condensed,
                    fontWeight = FontWeight.Bold,
                    fontSize = 30.sp,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.padding(top = 8.dp),
                )
                Text("Piece AWB · ${state.scanType.label}", color = c.muted, fontSize = 14.sp)
            }
        }

        // ── Context fields ───────────────────────────────────────────────────
        Column(
            modifier = Modifier.padding(start = 16.dp, end = 16.dp, top = 16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            MoveTextField(value = state.hubId, onValueChange = viewModel::setHubId, label = "Hub ID *",
                placeholder = "UUID of this hub", mono = true)
            MoveTextField(value = state.masterAwb, onValueChange = viewModel::setMasterAwb, label = "Master AWB *",
                placeholder = "e.g. CM-PHL-S0012345", mono = true)
            ShipmentIdField(
                value         = state.shipmentId,
                isResolving   = state.isResolvingShipment,
                resolveFailed = state.shipmentResolveFailed,
                onValueChange = viewModel::setShipmentId,
            )
            MoveTextField(value = state.pieceAwb, onValueChange = {
                viewModel.onPieceScan(ScanResult(it, "manual"), System.currentTimeMillis())
            }, label = "Piece AWB (scanned)", placeholder = "Scan, or type the child AWB", mono = true)
            if (state.scanType.requiresPallet) {
                MoveTextField(value = state.palletId, onValueChange = viewModel::setPalletId, label = "Pallet ID *",
                    placeholder = "UUID of the target pallet", mono = true)
            }
            if (state.scanType.requiresContainer) {
                MoveTextField(value = state.containerId, onValueChange = viewModel::setContainerId, label = "Container ID *",
                    placeholder = "UUID of the container / vehicle", mono = true)
            }
        }

        Spacer(Modifier.height(16.dp))

        // ── Submit ───────────────────────────────────────────────────────────
        MoveBigButton(
            label = "SUBMIT SCAN",
            onClick = { viewModel.submitScan() },
            modifier = Modifier.padding(horizontal = 16.dp),
            enabled = state.canSubmit,
            loading = state.isSubmitting,
        )

        // ── Feedback ─────────────────────────────────────────────────────────
        AnimatedVisibility(visible = state.lastSubmitSuccess == true, enter = fadeIn(), exit = fadeOut()) {
            MoveNotice(
                title = if (state.lastSubmitQueued) "Queued offline" else "Scan recorded",
                body = if (state.lastSubmitQueued) "It syncs as soon as you're back online." else "Logged against the manifest.",
                tone = MoveTone.Accent,
                modifier = Modifier.padding(start = 16.dp, end = 16.dp, top = 12.dp),
            )
        }
        AnimatedVisibility(visible = state.error != null, enter = fadeIn(), exit = fadeOut()) {
            MoveNotice(
                title = "Scan not recorded",
                body = state.error ?: "",
                tone = MoveTone.Penalty,
                modifier = Modifier.padding(start = 16.dp, end = 16.dp, top = 12.dp),
            )
        }
    }
}

// ── Exception sub-types (shown only for EXCEPTION_FLAG) ───────────────────────

private val exceptionOptions = listOf(
    "missing"         to "Missing",
    "damaged"         to "Damaged",
    "weight_mismatch" to "Weight Mismatch",
)

// ── Shipment ID field with auto-resolve feedback ──────────────────────────────

@Composable
private fun ShipmentIdField(
    value:         String,
    isResolving:   Boolean,
    resolveFailed: Boolean,
    onValueChange: (String) -> Unit,
) {
    val c = LocalMoveColors.current
    val trailing: (@Composable () -> Unit)? = when {
        isResolving -> {
            { CircularProgressIndicator(modifier = Modifier.size(18.dp), color = c.accent, strokeWidth = 2.dp) }
        }
        value.isNotBlank() && !resolveFailed -> {
            { Icon(Icons.Filled.CheckCircle, contentDescription = null, tint = c.success, modifier = Modifier.size(20.dp)) }
        }
        resolveFailed -> {
            { Icon(Icons.Filled.Warning, contentDescription = null, tint = c.amber, modifier = Modifier.size(20.dp)) }
        }
        else -> null
    }
    MoveTextField(
        value = value,
        onValueChange = onValueChange,
        label = "Shipment ID *",
        placeholder = when {
            isResolving   -> "Resolving…"
            resolveFailed -> "AWB not found — enter manually"
            else          -> "Scan master AWB to auto-fill"
        },
        enabled = !isResolving,
        mono = true,
        supportingText = if (resolveFailed) "The master AWB didn't match a shipment. Type its ID." else null,
        trailingIcon = trailing,
    )
}

// ── Camera section ────────────────────────────────────────────────────────────

@Composable
private fun CameraSection(
    onScanResult: (ScanResult) -> Unit,
) {
    val c = LocalMoveColors.current
    val context       = LocalContext.current
    val lifecycleOwner = LocalLifecycleOwner.current
    val hasCameraPermission = remember(context) {
        ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) ==
                PackageManager.PERMISSION_GRANTED
    }

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
        Box(Modifier.fillMaxSize().padding(24.dp), contentAlignment = Alignment.Center) {
            Text("Camera permission is off — type the AWB below.", color = c.muted, fontSize = 14.sp)
        }
    }
}
