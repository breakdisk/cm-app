package io.logisticos.driver.feature.boxmeasure.ui

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.*
import androidx.compose.foundation.*
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlin.math.roundToInt
import androidx.core.content.ContextCompat
import androidx.hilt.navigation.compose.hiltViewModel
import io.logisticos.driver.feature.boxmeasure.data.*
import io.logisticos.driver.feature.boxmeasure.presentation.*

// ── Design tokens ──────────────────────────────────────────────────────────────
private val Canvas  = Color(0xFF050810)
private val Cyan    = Color(0xFF00E5FF)
private val Purple  = Color(0xFFA855F7)
private val Green   = Color(0xFF00FF88)
private val Amber   = Color(0xFFFFAB00)
private val Red     = Color(0xFFFF3B5C)
private val Glass   = Color(0x0AFFFFFF)
private val Border  = Color(0x14FFFFFF)
private val TextPrimary = Color(0xFFE2E8F0)
private val TextMuted   = Color(0xFF64748B)

private fun componentColor(c: QuoteLine.Component) = when (c) {
    QuoteLine.Component.SEA         -> Cyan
    QuoteLine.Component.AIR         -> Purple
    QuoteLine.Component.PH_DELIVERY -> Green
}

/**
 * BoxMeasureScreen — dual-purpose screen:
 *
 * Mode VERIFY: shows declared dimensions alongside AR-measured dimensions.
 *              "Confirm" button sends results back via [onDimensionsVerified].
 *
 * Mode QUOTE:  standalone quote flow for walk-in customers.
 *              Displays full quote result with origin / province selection.
 *              After the price is shown, a "Book This Shipment" button calls
 *              [onBookShipment] with all relevant booking parameters.
 *
 * @param mode                 VERIFY or QUOTE
 * @param declaredSizeId       (VERIFY) size_id from the booking
 * @param declaredL/W/H        (VERIFY) declared L×W×H in cm
 * @param onDimensionsVerified (VERIFY) called with confirmed L, W, H
 * @param onBookShipment       (QUOTE) called when driver taps "Book This Shipment"
 * @param onBack               navigation pop
 */
@Composable
fun BoxMeasureScreen(
    mode: BoxMeasureMode = BoxMeasureMode.QUOTE,
    declaredSizeId: String? = null,
    declaredL: Double? = null,
    declaredW: Double? = null,
    declaredH: Double? = null,
    onDimensionsVerified: ((PopDimensioning) -> Unit)? = null,
    onBookShipment: ((sizeId: String, freightMode: FreightMode, originKey: String,
                      l: Double, w: Double, h: Double,
                      estimatedTotal: Double, currency: String) -> Unit)? = null,
    onBack: () -> Unit = {},
    viewModel: BoxMeasureViewModel = hiltViewModel(),
) {
    val state by viewModel.uiState.collectAsState()
    val context = LocalContext.current

    // One-click "full display": expands the AR viewport to an immersive full-screen
    // layer over the form. Drivers measure in the larger viewport (a bigger target
    // area = steadier tracking) then collapse back to the quote inputs.
    var arExpanded by rememberSaveable { mutableStateOf(false) }

    // ── Camera permission ─────────────────────────────────────────────────────
    // ARCore requires CAMERA at runtime. Request it before the GLSurfaceView
    // initialises — Session.resume() throws CameraNotAvailableException (unchecked
    // on the GL thread) if permission isn't granted, crashing the app.
    var cameraGranted by remember {
        mutableStateOf(
            ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA)
                == PackageManager.PERMISSION_GRANTED
        )
    }
    val cameraLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestPermission()
    ) { granted ->
        cameraGranted = granted
        if (!granted) {
            viewModel.onArSessionError("Camera permission denied — use manual entry.")
            viewModel.setManualMode(true)
        }
    }
    LaunchedEffect(Unit) {
        if (!cameraGranted) cameraLauncher.launch(Manifest.permission.CAMERA)
        if (mode == BoxMeasureMode.VERIFY) {
            viewModel.initVerifyMode(declaredSizeId, declaredL, declaredW, declaredH)
        }
    }

    // Auto-switch to manual mode when AR session errors (device not supported,
    // camera unavailable, ARCore not installed, etc.).
    LaunchedEffect(state.measureError) {
        if (state.measureError != null && !state.manualMode) {
            viewModel.setManualMode(true)
        }
    }

    // Handle VERIFY confirm callback
    LaunchedEffect(state.dimensionConfirmed) {
        if (state.dimensionConfirmed) {
            onDimensionsVerified?.invoke(viewModel.popDimensioning())
            onBack()
        }
    }

    Box(modifier = Modifier.fillMaxSize().background(Canvas)) {
    if (cameraGranted && state.measureError == null && arExpanded) {
        // Full-screen AR replaces the form entirely while expanded. The scroll form
        // — and its dropdown popups — are not composed, so nothing floats over the
        // camera and per-tap recomposition stays cheap (fixes the dropdown leak and
        // the measuring lag).
        ArViewport(
            viewModel    = viewModel,
            state        = state,
            expanded     = true,
            onToggleFull = { arExpanded = false },
        )
    } else {
    Column(
        modifier = Modifier.fillMaxSize()
    ) {
        // ── Top bar ─────────────────────────────────────────────────────────────
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 16.dp, vertical = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            IconButton(onClick = onBack) {
                Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back", tint = TextPrimary)
            }
            Column(modifier = Modifier.weight(1f).padding(start = 4.dp)) {
                Text(
                    text = if (mode == BoxMeasureMode.VERIFY) "Verify Box Dimensions" else "Box Quote",
                    color = TextPrimary, fontWeight = FontWeight.Bold, fontSize = 16.sp,
                )
                Text(
                    text = if (mode == BoxMeasureMode.VERIFY) "Compare declared vs. measured"
                           else "Instant price estimate",
                    color = TextMuted, fontSize = 11.sp,
                )
            }
            if (state.arSessionReady) {
                Surface(
                    shape = RoundedCornerShape(8.dp),
                    color = Cyan.copy(alpha = 0.1f),
                    border = BorderStroke(1.dp, Cyan.copy(alpha = 0.25f)),
                ) {
                    Row(
                        modifier = Modifier.padding(horizontal = 8.dp, vertical = 4.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(4.dp),
                    ) {
                        Icon(Icons.Default.ViewInAr, contentDescription = null, tint = Cyan, modifier = Modifier.size(12.dp))
                        Text("AR Ready", color = Cyan, fontSize = 10.sp, fontWeight = FontWeight.Bold)
                    }
                }
            }
        }

        Column(
            modifier = Modifier
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 16.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {

            // ── AR error banner ───────────────────────────────────────────────
            // Shown when the session failed (device not supported, camera denied,
            // ARCore not installed). Manual mode is auto-enabled at this point.
            if (state.measureError != null) {
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .clip(RoundedCornerShape(12.dp))
                        .background(Amber.copy(alpha = 0.12f))
                        .border(1.dp, Amber.copy(alpha = 0.4f), RoundedCornerShape(12.dp))
                        .padding(12.dp),
                    horizontalArrangement = Arrangement.spacedBy(10.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Icon(Icons.Default.WarningAmber, contentDescription = null, tint = Amber, modifier = Modifier.size(18.dp))
                    Text(state.measureError!!, color = Amber, fontSize = 12.sp, modifier = Modifier.weight(1f))
                }
            }

            // ── AR camera view (full-width, 260dp tall) ────────────────────────
            // Only rendered when camera permission is granted and no session error.
            // On unsupported devices or permission denial, the error banner above
            // explains the situation and manual mode is enabled automatically.
            //
            // The full-screen variant is handled above (it replaces the whole form),
            // so this inline slot only ever renders the compact viewport.
            if (cameraGranted && state.measureError == null) {
                Box(
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(260.dp)
                        .clip(RoundedCornerShape(16.dp))
                        .border(1.dp, Border, RoundedCornerShape(16.dp)),
                ) {
                    ArViewport(
                        viewModel    = viewModel,
                        state        = state,
                        expanded     = false,
                        onToggleFull = { arExpanded = true },
                    )
                }
            }

            // ── Manual entry toggle ────────────────────────────────────────────
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text("Enter manually", color = TextMuted, fontSize = 12.sp)
                Switch(
                    checked = state.manualMode,
                    onCheckedChange = viewModel::setManualMode,
                    colors = SwitchDefaults.colors(checkedThumbColor = Cyan, checkedTrackColor = Cyan.copy(alpha = 0.3f)),
                )
            }

            if (state.manualMode) {
                ManualDimensionEntry(
                    l = state.manualL, w = state.manualW, h = state.manualH,
                    onLChange = viewModel::setManualL,
                    onWChange = viewModel::setManualW,
                    onHChange = viewModel::setManualH,
                    onApply   = viewModel::applyManualDimensions,
                )
            }

            // ── Measurement result card ────────────────────────────────────────
            // Local val required: smart cast on a delegated property (StateFlow) is
            // impossible — the backing getter can change between the null check and use.
            val measuredL = state.measuredL
            if (measuredL != null) {
                MeasurementCard(
                    measuredL    = measuredL,
                    measuredW    = state.measuredW ?: 0.0,
                    measuredH    = state.measuredH ?: 0.0,
                    confidence   = state.arConfidence,
                    integrity    = state.integrity,
                    integrityReason = state.integrityReason,
                    overrideUsed = state.manualOverrideUsed,
                    declaredL    = if (mode == BoxMeasureMode.VERIFY) state.declaredL else null,
                    declaredW    = if (mode == BoxMeasureMode.VERIFY) state.declaredW else null,
                    declaredH    = if (mode == BoxMeasureMode.VERIFY) state.declaredH else null,
                )
            }

            // ── Standard box size selector ─────────────────────────────────────
            SectionLabel("Standard Box Size")
            BoxSizeSelector(
                sizes      = BOX_SIZES,
                selectedId = state.matchedSizeId,
                onSelect   = viewModel::setMatchedSizeId,
            )

            Spacer(Modifier.height(4.dp))

            // ── VERIFY mode: Confirm button ────────────────────────────────────
            if (mode == BoxMeasureMode.VERIFY) {
                VerifyConfirmSection(
                    hasResult   = state.measuredL != null,
                    integrity   = state.integrity,
                    qty         = state.qty,
                    onQtyChange = viewModel::setQty,
                    onConfirm   = viewModel::confirmDimensions,
                )
            }

            // ── QUOTE mode: quote inputs + result ─────────────────────────────
            if (mode == BoxMeasureMode.QUOTE) {
                QuoteInputSection(
                    state          = state,
                    viewModel      = viewModel,
                    onBookShipment = onBookShipment,
                )
            }

            Spacer(Modifier.height(32.dp))
        }
    }
    }
    }
}

// ── Sub-composables ────────────────────────────────────────────────────────────

/**
 * Live measurement reticle — replaces the raw X/Y/Z world-coordinate display.
 *
 * Shows a crosshair `+` and a coloured chip whose value is the **Euclidean distance
 * in cm** from the last placed anchor to the current camera aim point, updating at
 * ~6 Hz.  Colour and axis label encode which edge the driver is measuring:
 *
 *   No anchor yet  — muted  — "Tap to place anchor"
 *   Tap 1 placed   — cyan   — LENGTH  · XX.X cm
 *   Tap 2 placed   — pink   — WIDTH   · XX.X cm
 *   Tap 3 placed   — green  — HEIGHT  · XX.X cm
 *
 * This gives the driver real-time feedback that the AR plane is tracking (the number
 * changes as they move) and tells them exactly what they're measuring before they tap.
 */
@Composable
private fun LiveMeasureReticle(
    distanceCm: Double?,
    tapCount: Int,
    modifier: Modifier = Modifier,
) {
    // Axis label + accent colour keyed by how many anchors have been placed.
    val (axisLabel, accentColor) = when (tapCount) {
        1    -> "LENGTH" to Cyan
        2    -> "WIDTH"  to Color(0xFFFF2D78)   // matches the DimAxis.WIDTH chip
        3    -> "HEIGHT" to Green
        else -> null     to TextMuted            // 0 = no anchor placed yet
    }

    Column(
        modifier = modifier,
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        // Crosshair — coloured by current axis for instant visual association.
        androidx.compose.foundation.Canvas(modifier = Modifier.size(20.dp)) {
            val c      = size.width / 2f
            val arm    = 8.dp.toPx()
            val stroke = 1.5.dp.toPx()
            // Shadow pass (offset 1 px) then accent — keeps it legible over bright floors.
            listOf(Color.Black.copy(alpha = 0.4f) to 1f, accentColor to 0f).forEach { (col, off) ->
                drawLine(col, Offset(c - arm + off, c + off), Offset(c + arm + off, c + off), stroke)
                drawLine(col, Offset(c + off, c - arm + off), Offset(c + off, c + arm + off), stroke)
            }
        }

        // Live readout chip.
        Column(
            modifier = Modifier
                .background(Color(0xCC050810), RoundedCornerShape(10.dp))
                .border(1.dp, accentColor.copy(alpha = 0.55f), RoundedCornerShape(10.dp))
                .padding(horizontal = 14.dp, vertical = 9.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(3.dp),
        ) {
            if (axisLabel == null) {
                // No anchor placed — primary instruction.
                Text(
                    "Tap to place anchor",
                    color      = accentColor,
                    fontSize   = 12.sp,
                    fontWeight = FontWeight.SemiBold,
                )
            } else {
                // Axis label (small caps above the measurement value).
                Text(
                    axisLabel,
                    color         = accentColor,
                    fontSize      = 9.sp,
                    fontWeight    = FontWeight.Bold,
                    fontFamily    = FontFamily.Monospace,
                    letterSpacing = 1.sp,
                )
                // Live cm value — large and bold so it is readable at arm's length.
                Text(
                    if (distanceCm != null) "${"%.1f".format(distanceCm)} cm" else "— cm",
                    color      = if (distanceCm != null) Color.White else TextMuted,
                    fontSize   = 20.sp,
                    fontWeight = FontWeight.ExtraBold,
                    fontFamily = FontFamily.Monospace,
                )
            }
        }
    }
}


/**
 * Live AR viewport: camera background + aiming crosshair + 3-stage guide + the
 * full-display toggle and (once captured) the integrity badge.
 *
 * Used in exactly one place at a time — inline (260dp) or in the full-screen
 * layer — so only a single ARCore session is mounted.
 */
@Composable
private fun ArViewport(
    viewModel: BoxMeasureViewModel,
    state: BoxMeasureUiState,
    expanded: Boolean,
    onToggleFull: () -> Unit,
) {
    Box(modifier = Modifier.fillMaxSize()) {
        ArCoreBoxMeasureView(
            modifier = Modifier.fillMaxSize(),
            viewModel = viewModel,
        )

        // Top scrim — keeps the cyan guide + controls legible over a bright camera feed.
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .height(if (expanded) 168.dp else 96.dp)
                .background(Brush.verticalGradient(listOf(Color(0xCC050810), Color.Transparent)))
                .align(Alignment.TopCenter),
        )

        // Centre measurement reticle — shown until all 4 anchors are placed.
        // Displays a live per-axis distance in cm from the last placed anchor to
        // the current aim point so the driver sees the measurement grow in real time.
        if (state.tapCount < 4) {
            LiveMeasureReticle(
                distanceCm = state.liveDistanceCm,
                tapCount   = state.tapCount,
                modifier   = Modifier.align(Alignment.Center),
            )
        }

        // 3-stage guide (top-start).
        MeasureGuideOverlay(
            tapCount = state.tapCount,
            modifier = Modifier
                .align(Alignment.TopStart)
                .padding(top = if (expanded) 48.dp else 12.dp, start = 12.dp, end = 12.dp),
        )

        // Full-display toggle (top-end).
        Surface(
            onClick = onToggleFull,
            shape = RoundedCornerShape(10.dp),
            color = Color(0xCC050810),
            border = BorderStroke(1.dp, Cyan.copy(alpha = 0.3f)),
            modifier = Modifier
                .align(Alignment.TopEnd)
                .padding(top = if (expanded) 48.dp else 12.dp, end = 12.dp),
        ) {
            Row(
                modifier = Modifier.padding(horizontal = 10.dp, vertical = 6.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                Icon(
                    if (expanded) Icons.Default.FullscreenExit else Icons.Default.Fullscreen,
                    contentDescription = if (expanded) "Exit full screen" else "Full display",
                    tint = Cyan, modifier = Modifier.size(16.dp),
                )
                Text(if (expanded) "Exit" else "Full", color = Cyan, fontSize = 11.sp, fontWeight = FontWeight.Bold)
            }
        }

        // Integrity badge (bottom-start) — appears once a measurement is captured.
        if (state.integrity != MeasurementIntegrity.PENDING) {
            IntegrityBadge(
                integrity = state.integrity,
                modifier = Modifier
                    .align(Alignment.BottomStart)
                    .padding(start = 12.dp, bottom = if (expanded) 32.dp else 12.dp),
            )
        }

        // Re-measure (bottom-end) — drops captured points and restarts the scan.
        // Visible as soon as the first anchor is placed (and after completion).
        if (state.tapCount > 0) {
            Surface(
                onClick = { viewModel.resetMeasurement() },
                shape = RoundedCornerShape(10.dp),
                color = Color(0xCC050810),
                border = BorderStroke(1.dp, Amber.copy(alpha = 0.4f)),
                modifier = Modifier
                    .align(Alignment.BottomEnd)
                    .padding(end = 12.dp, bottom = if (expanded) 32.dp else 12.dp),
            ) {
                Row(
                    modifier = Modifier.padding(horizontal = 10.dp, vertical = 6.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(4.dp),
                ) {
                    Icon(Icons.Default.Refresh, contentDescription = "Re-measure", tint = Amber, modifier = Modifier.size(14.dp))
                    Text("Re-measure", color = Amber, fontSize = 11.sp, fontWeight = FontWeight.Bold)
                }
            }
        }

        // AR dimension chips — float over each measured edge midpoint (positions are
        // projected to screen pixels by the renderer), value shown in inches.
        state.dimLabels.forEach { label ->
            DimChip(label = label, modifier = Modifier.align(Alignment.TopStart))
        }
    }
}

/** Floating colored dimension chip positioned at a projected edge midpoint. */
@Composable
private fun DimChip(label: DimLabel, modifier: Modifier = Modifier) {
    val color = when (label.axis) {
        DimAxis.LENGTH -> Cyan
        DimAxis.WIDTH  -> Color(0xFFFF2D78)
        DimAxis.HEIGHT -> Green
    }
    val inches = label.cm * 0.393701
    Box(
        modifier = modifier.offset {
            // Centre the chip on the projected point (approx half-size offset).
            IntOffset(
                (label.xPx - 30.dp.toPx()).roundToInt(),
                (label.yPx - 14.dp.toPx()).roundToInt(),
            )
        },
    ) {
        Text(
            "%.1fin".format(inches),
            color = Color.White,
            fontSize = 13.sp,
            fontWeight = FontWeight.Bold,
            fontFamily = FontFamily.Monospace,
            modifier = Modifier
                .background(color, RoundedCornerShape(6.dp))
                .padding(horizontal = 8.dp, vertical = 3.dp),
        )
    }
}

/**
 * 3-stage AR guidance: place anchor → drag to resize → view dimensions.
 *
 * The underlying ARCore capture is still a 4-point pick; the three stages are the
 * user-facing model. Stage 2 ("Drag to resize") carries a per-edge sub-hint as the
 * driver works through length / width / height.
 */
@Composable
private fun MeasureGuideOverlay(tapCount: Int, modifier: Modifier = Modifier) {
    val stages = listOf("Tap to place anchor", "Drag to resize", "View dimensions")
    val stage = when {
        tapCount <= 0 -> 0
        tapCount < 4  -> 1
        else          -> 2
    }
    val resizeHint = when (tapCount) {
        1 -> "Length"; 2 -> "Width"; 3 -> "Height"; else -> null
    }
    Column(
        modifier = modifier
            .background(Color(0xCC050810), RoundedCornerShape(12.dp))
            .padding(horizontal = 14.dp, vertical = 10.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(
                "Step ${stage + 1} / 3",
                color = Cyan, fontSize = 11.sp, fontWeight = FontWeight.Bold, fontFamily = FontFamily.Monospace,
            )
            Text(
                stages[stage] + (resizeHint?.let { " · $it" } ?: ""),
                color = Color.White, fontSize = 13.sp, fontWeight = FontWeight.SemiBold,
            )
        }
        Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            repeat(3) { i ->
                Box(
                    modifier = Modifier
                        .size(width = if (i == stage) 36.dp else 18.dp, height = 4.dp)
                        .clip(RoundedCornerShape(2.dp))
                        .background(if (i <= stage) Cyan else Border),
                )
            }
        }
    }
}

/** Compact anti-fraud chip reflecting the measurement integrity score. */
@Composable
private fun IntegrityBadge(integrity: MeasurementIntegrity, modifier: Modifier = Modifier) {
    val (color, icon, label) = integrityVisual(integrity) ?: return
    Surface(
        shape = RoundedCornerShape(8.dp),
        color = Color(0xCC050810),
        border = BorderStroke(1.dp, color.copy(alpha = 0.4f)),
        modifier = modifier,
    ) {
        Row(
            modifier = Modifier.padding(horizontal = 8.dp, vertical = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            Icon(icon, null, tint = color, modifier = Modifier.size(12.dp))
            Text(label, color = color, fontSize = 10.sp, fontWeight = FontWeight.Bold)
        }
    }
}

/** Maps an integrity score to (accent color, icon, label). Null for PENDING. */
private fun integrityVisual(integrity: MeasurementIntegrity): Triple<Color, androidx.compose.ui.graphics.vector.ImageVector, String>? =
    when (integrity) {
        MeasurementIntegrity.VERIFIED -> Triple(Green, Icons.Default.VerifiedUser, "Integrity verified")
        MeasurementIntegrity.REVIEW   -> Triple(Amber, Icons.Default.Shield, "Needs review")
        MeasurementIntegrity.FLAGGED  -> Triple(Red,   Icons.Default.ReportProblem, "Flagged")
        MeasurementIntegrity.PENDING  -> null
    }

@Composable
private fun MeasurementCard(
    measuredL: Double, measuredW: Double, measuredH: Double, confidence: Double,
    integrity: MeasurementIntegrity, integrityReason: String?, overrideUsed: Boolean,
    declaredL: Double?, declaredW: Double?, declaredH: Double?,
) {
    val cbm = computeCbm(measuredL, measuredW, measuredH)
    val accent = when (integrity) {
        MeasurementIntegrity.VERIFIED -> Green
        MeasurementIntegrity.REVIEW   -> Amber
        MeasurementIntegrity.FLAGGED  -> Red
        MeasurementIntegrity.PENDING  -> Cyan
    }

    Surface(
        color = Glass,
        shape = RoundedCornerShape(16.dp),
        border = BorderStroke(1.dp, accent.copy(alpha = 0.35f)),
    ) {
        Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Icon(Icons.Default.ViewInAr, null, tint = Cyan, modifier = Modifier.size(16.dp))
                Text("Measured Dimensions", color = Cyan, fontWeight = FontWeight.Bold, fontSize = 13.sp)
                Spacer(Modifier.weight(1f))
                Text(
                    "${(confidence * 100).toInt()}% confidence",
                    color = if (confidence > 0.8) Green else Amber,
                    fontSize = 10.sp, fontFamily = FontFamily.Monospace,
                )
            }

            // Dimension rows
            listOf(
                Triple("Length", measuredL, declaredL),
                Triple("Width",  measuredW, declaredW),
                Triple("Height", measuredH, declaredH),
            ).forEach { (label, measured, declared) ->
                DimensionRow(label, measured, declared)
            }

            // CBM
            Divider(color = Border, thickness = 1.dp)
            Row(horizontalArrangement = Arrangement.SpaceBetween, modifier = Modifier.fillMaxWidth()) {
                Text("Volume (CBM)", color = TextMuted, fontSize = 12.sp)
                Text("$cbm m³", color = Purple, fontSize = 12.sp, fontFamily = FontFamily.Monospace, fontWeight = FontWeight.Bold)
            }

            // Volumetric (dimensional) weight — what a bulky-but-light box is charged
            // at on air freight (billed on the greater of actual vs. volumetric).
            val volWeight = computeVolumetricWeight(measuredL, measuredW, measuredH)
            Row(
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier.fillMaxWidth(),
            ) {
                Column {
                    Text("Volumetric weight", color = TextMuted, fontSize = 12.sp)
                    Text("L×W×H ÷ ${VOLUMETRIC_DIVISOR.toInt()}", color = TextMuted.copy(alpha = 0.6f),
                        fontSize = 9.sp, fontFamily = FontFamily.Monospace)
                }
                Text("${"%.1f".format(volWeight)} kg", color = Amber, fontSize = 12.sp,
                    fontFamily = FontFamily.Monospace, fontWeight = FontWeight.Bold)
            }

            // ── Anti-fraud integrity row ──────────────────────────────────────────
            integrityVisual(integrity)?.let { (color, icon, label) ->
                Divider(color = Border, thickness = 1.dp)
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .clip(RoundedCornerShape(10.dp))
                        .background(color.copy(alpha = 0.10f))
                        .padding(10.dp),
                    verticalAlignment = Alignment.Top,
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    Icon(icon, null, tint = color, modifier = Modifier.size(16.dp))
                    Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                        Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
                            Text(label, color = color, fontWeight = FontWeight.Bold, fontSize = 12.sp)
                            Text(
                                if (overrideUsed) "· manual entry" else "· AR scan",
                                color = TextMuted, fontSize = 10.sp, fontFamily = FontFamily.Monospace,
                            )
                        }
                        Text(
                            integrityReason ?: when (integrity) {
                                MeasurementIntegrity.VERIFIED ->
                                    "Tamper-evident: captured on-device with AR depth tracking."
                                else -> "Recorded for audit."
                            },
                            color = TextMuted, fontSize = 11.sp, lineHeight = 15.sp,
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun DimensionRow(label: String, measured: Double, declared: Double?) {
    val discrepancy = declared?.let { kotlin.math.abs(measured - it) } ?: 0.0
    val overTolerance = discrepancy > (declared ?: 0.0) * 0.05 && declared != null

    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(label, color = TextMuted, fontSize = 12.sp, modifier = Modifier.weight(1f))
        Text(
            "${"%.1f".format(measured)} cm",
            color = TextPrimary, fontSize = 13.sp,
            fontFamily = FontFamily.Monospace, fontWeight = FontWeight.Bold,
        )
        if (declared != null) {
            Text(
                " (declared ${"%.0f".format(declared)})",
                color = if (overTolerance) Amber else TextMuted,
                fontSize = 10.sp,
            )
            if (overTolerance) {
                Icon(Icons.Default.Warning, null, tint = Amber, modifier = Modifier.size(12.dp).padding(start = 2.dp))
            }
        }
    }
}

@Composable
private fun BoxSizeSelector(sizes: List<BoxSize>, selectedId: String, onSelect: (String) -> Unit) {
    LazyRow(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        items(sizes) { size ->
            val selected = size.id == selectedId
            Surface(
                onClick = { onSelect(size.id) },
                shape  = RoundedCornerShape(12.dp),
                color  = if (selected) Cyan.copy(alpha = 0.08f) else Glass,
                border = BorderStroke(1.dp, if (selected) Cyan.copy(alpha = 0.5f) else Border),
            ) {
                Column(
                    modifier = Modifier.padding(horizontal = 14.dp, vertical = 10.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.spacedBy(4.dp),
                ) {
                    Text(size.name, color = if (selected) Cyan else TextPrimary, fontWeight = FontWeight.Bold, fontSize = 13.sp)
                    Text(size.dimensions, color = TextMuted, fontSize = 9.sp)
                    Text("${size.cbm} m³", color = Purple, fontSize = 9.sp, fontFamily = FontFamily.Monospace)
                    Text("≤${size.maxWeightKg}kg", color = TextMuted, fontSize = 9.sp)
                }
            }
        }
    }
}

@Composable
private fun ManualDimensionEntry(
    l: String, w: String, h: String,
    onLChange: (String) -> Unit, onWChange: (String) -> Unit, onHChange: (String) -> Unit,
    onApply: () -> Unit,
) {
    Surface(color = Glass, shape = RoundedCornerShape(14.dp), border = BorderStroke(1.dp, Border)) {
        Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Text("Enter dimensions (cm)", color = TextMuted, fontSize = 12.sp)
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                listOf("L" to (l to onLChange), "W" to (w to onWChange), "H" to (h to onHChange))
                    .forEach { (label, pair) ->
                        val (value, onChange) = pair
                        Column(modifier = Modifier.weight(1f)) {
                            Text(label, color = TextMuted, fontSize = 10.sp, modifier = Modifier.padding(bottom = 4.dp))
                            OutlinedTextField(
                                value = value, onValueChange = onChange,
                                modifier = Modifier.fillMaxWidth(),
                                placeholder = { Text("0", color = TextMuted, fontSize = 12.sp) },
                                keyboardOptions = androidx.compose.foundation.text.KeyboardOptions(
                                    keyboardType = androidx.compose.ui.text.input.KeyboardType.Decimal
                                ),
                                colors = OutlinedTextFieldDefaults.colors(
                                    focusedBorderColor = Cyan,
                                    unfocusedBorderColor = Border,
                                    focusedTextColor = TextPrimary,
                                    unfocusedTextColor = TextPrimary,
                                    cursorColor = Cyan,
                                ),
                                singleLine = true,
                            )
                        }
                    }
            }
            Button(
                onClick = onApply,
                modifier = Modifier.fillMaxWidth(),
                colors = ButtonDefaults.buttonColors(containerColor = Cyan),
                shape = RoundedCornerShape(10.dp),
            ) {
                Text("Apply", color = Canvas, fontWeight = FontWeight.Bold)
            }
        }
    }
}

@Composable
private fun VerifyConfirmSection(
    hasResult: Boolean,
    integrity: MeasurementIntegrity,
    qty: Int,
    onQtyChange: (Int) -> Unit,
    onConfirm: () -> Unit,
) {
    // Box count — captured into the POP for the size/quantity audit.
    SectionLabel("Quantity of boxes")
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        QtyButton("−") { onQtyChange((qty - 1).coerceAtLeast(1)) }
        Text("$qty", color = TextPrimary, fontSize = 18.sp, fontWeight = FontWeight.Bold,
            fontFamily = FontFamily.Monospace, modifier = Modifier.widthIn(min = 32.dp), textAlign = TextAlign.Center)
        QtyButton("+") { onQtyChange((qty + 1).coerceAtMost(99)) }
        Spacer(Modifier.weight(1f))
        Text("identical box(es) in this pickup", color = TextMuted, fontSize = 11.sp)
    }
    Spacer(Modifier.height(4.dp))

    if (!hasResult) {
        Text(
            "Place the anchor and resize to measure with AR, or enter dimensions manually to confirm.",
            color = TextMuted, fontSize = 12.sp, textAlign = TextAlign.Center,
            modifier = Modifier.fillMaxWidth().padding(vertical = 8.dp),
        )
        return
    }

    // Anti-fraud gate: a flagged measurement cannot be confirmed onto the POP.
    val blocked = integrity == MeasurementIntegrity.FLAGGED
    if (blocked) {
        FraudBlockNote("Measurement flagged — confirmation is blocked. Re-scan with AR, or the hub will re-measure before billing.")
    }

    Button(
        onClick = onConfirm,
        enabled = !blocked,
        modifier = Modifier.fillMaxWidth().height(52.dp),
        shape = RoundedCornerShape(14.dp),
        colors = ButtonDefaults.buttonColors(containerColor = Color.Transparent, disabledContainerColor = Color.Transparent),
        contentPadding = PaddingValues(0.dp),
    ) {
        Box(
            modifier = Modifier
                .fillMaxSize()
                .background(
                    if (blocked) Brush.horizontalGradient(listOf(Border, Border))
                    else Brush.horizontalGradient(listOf(Cyan, Purple)),
                    RoundedCornerShape(14.dp),
                ),
            contentAlignment = Alignment.Center,
        ) {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                Icon(Icons.Default.Check, null, tint = if (blocked) TextMuted else Canvas)
                Text("Confirm Dimensions", color = if (blocked) TextMuted else Canvas, fontWeight = FontWeight.Bold, fontSize = 15.sp)
            }
        }
    }
}

@Composable
private fun QtyButton(glyph: String, onClick: () -> Unit) {
    Surface(
        onClick = onClick,
        shape = RoundedCornerShape(10.dp),
        color = Glass,
        border = BorderStroke(1.dp, Cyan.copy(alpha = 0.4f)),
        modifier = Modifier.size(40.dp),
    ) {
        Box(contentAlignment = Alignment.Center) {
            Text(glyph, color = Cyan, fontSize = 20.sp, fontWeight = FontWeight.Bold)
        }
    }
}

/** Prominent red note shown when a measurement is fraud-flagged. */
@Composable
private fun FraudBlockNote(message: String) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(12.dp))
            .background(Red.copy(alpha = 0.12f))
            .border(1.dp, Red.copy(alpha = 0.4f), RoundedCornerShape(12.dp))
            .padding(12.dp),
        horizontalArrangement = Arrangement.spacedBy(10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(Icons.Default.ReportProblem, null, tint = Red, modifier = Modifier.size(18.dp))
        Text(message, color = Red, fontSize = 12.sp, lineHeight = 16.sp, modifier = Modifier.weight(1f))
    }
}

@OptIn(androidx.compose.material3.ExperimentalMaterial3Api::class)
@Composable
private fun QuoteInputSection(
    state: BoxMeasureUiState,
    viewModel: BoxMeasureViewModel,
    onBookShipment: ((sizeId: String, freightMode: FreightMode, originKey: String,
                      l: Double, w: Double, h: Double,
                      estimatedTotal: Double, currency: String) -> Unit)? = null,
) {
    val context = LocalContext.current

    // Freight mode toggle
    SectionLabel("Freight Mode")
    Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        FreightMode.entries.forEach { m ->
            val selected = state.freightMode == m
            Surface(
                onClick = { viewModel.setFreightMode(m) },
                shape  = RoundedCornerShape(12.dp),
                color  = if (selected) Cyan.copy(alpha = 0.1f) else Glass,
                border = BorderStroke(1.dp, if (selected) Cyan.copy(alpha = 0.4f) else Border),
                modifier = Modifier.weight(1f),
            ) {
                Row(
                    modifier = Modifier.padding(12.dp),
                    horizontalArrangement = Arrangement.Center,
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Icon(
                        if (m == FreightMode.SEA) Icons.Default.DirectionsBoat else Icons.Default.Flight,
                        null, tint = if (selected) Cyan else TextMuted,
                        modifier = Modifier.size(16.dp),
                    )
                    Spacer(Modifier.width(6.dp))
                    Text(m.name.lowercase().replaceFirstChar { it.uppercase() },
                        color = if (selected) Cyan else TextMuted, fontWeight = FontWeight.Bold, fontSize = 13.sp)
                }
            }
        }
    }

    // Origin selector
    SectionLabel("Origin")
    val origins = if (state.freightMode == FreightMode.SEA)
        SEA_CARGO.map { it.origin } else AIR_ZONES.map { it.zoneName }
    var expanded by remember { mutableStateOf(false) }
    ExposedDropdownMenuBox(expanded = expanded, onExpandedChange = { expanded = it }) {
        OutlinedTextField(
            value = state.originKey,
            onValueChange = {},
            readOnly = true,
            modifier = Modifier.menuAnchor().fillMaxWidth(),
            trailingIcon = { ExposedDropdownMenuDefaults.TrailingIcon(expanded) },
            colors = OutlinedTextFieldDefaults.colors(
                focusedBorderColor = Cyan, unfocusedBorderColor = Border,
                focusedTextColor = TextPrimary, unfocusedTextColor = TextPrimary,
            ),
            singleLine = true,
        )
        ExposedDropdownMenu(expanded = expanded, onDismissRequest = { expanded = false },
            modifier = Modifier.background(Color(0xFF0A0F1E))) {
            origins.forEach { o ->
                DropdownMenuItem(
                    text = { Text(o, color = TextPrimary, fontSize = 12.sp, maxLines = 2, overflow = TextOverflow.Ellipsis) },
                    onClick = { viewModel.setOriginKey(o); expanded = false },
                )
            }
        }
    }

    // Province
    SectionLabel("PH Destination Province")
    OutlinedTextField(
        value = state.province,
        onValueChange = viewModel::setProvince,
        modifier = Modifier.fillMaxWidth(),
        placeholder = { Text("e.g. Cebu, Davao, Metro Manila…", color = TextMuted, fontSize = 12.sp) },
        colors = OutlinedTextFieldDefaults.colors(
            focusedBorderColor = Cyan, unfocusedBorderColor = Border,
            focusedTextColor = TextPrimary, unfocusedTextColor = TextPrimary,
        ),
        singleLine = true,
    )
    resolveProvince(state.province)?.let { entry ->
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            Box(Modifier.size(6.dp).background(Green, shape = RoundedCornerShape(3.dp)))
            Text("${entry.zoneCode} matched", color = Green, fontSize = 11.sp)
        }
    }

    // Calculate button
    Button(
        onClick = viewModel::calculateQuote,
        enabled = !state.isCalculating,
        modifier = Modifier.fillMaxWidth().height(52.dp),
        shape = RoundedCornerShape(14.dp),
        colors = ButtonDefaults.buttonColors(containerColor = Color.Transparent),
        contentPadding = PaddingValues(0.dp),
    ) {
        Box(
            modifier = Modifier
                .fillMaxSize()
                .background(
                    if (!state.isCalculating) Brush.horizontalGradient(listOf(Cyan, Purple))
                    else Brush.horizontalGradient(listOf(Border, Border)),
                    RoundedCornerShape(14.dp),
                ),
            contentAlignment = Alignment.Center,
        ) {
            if (state.isCalculating) {
                CircularProgressIndicator(color = Cyan, modifier = Modifier.size(20.dp), strokeWidth = 2.dp)
            } else {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Icon(Icons.Default.Calculate, null, tint = Canvas)
                    Text("Calculate Price", color = Canvas, fontWeight = FontWeight.Bold, fontSize = 15.sp)
                }
            }
        }
    }

    // Quote result
    state.quoteResult?.let { result ->
        Spacer(Modifier.height(4.dp))
        SectionLabel("Quote Breakdown")

        result.lines.forEach { line ->
            Surface(color = Glass, shape = RoundedCornerShape(12.dp), border = BorderStroke(1.dp, Border),
                modifier = Modifier.fillMaxWidth().padding(bottom = 6.dp)) {
                Row(modifier = Modifier.padding(12.dp), horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically) {
                    Column(modifier = Modifier.weight(1f).padding(end = 8.dp)) {
                        Text(line.label, color = componentColor(line.component), fontWeight = FontWeight.SemiBold, fontSize = 13.sp)
                        line.note?.let { Text(it, color = TextMuted, fontSize = 10.sp, maxLines = 2) }
                    }
                    Text(formatAmount(line.amount, line.currency),
                        color = TextPrimary, fontWeight = FontWeight.Bold, fontSize = 13.sp,
                        fontFamily = FontFamily.Monospace)
                }
            }
        }

        // Total
        Surface(
            color = Cyan.copy(alpha = 0.06f),
            shape = RoundedCornerShape(16.dp),
            border = BorderStroke(1.dp, Cyan.copy(alpha = 0.25f)),
            modifier = Modifier.fillMaxWidth(),
        ) {
            Row(
                modifier = Modifier.padding(16.dp),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Column {
                    Text("Estimated Total", color = TextMuted, fontSize = 11.sp)
                    Text("${result.originCurrency} · incl. PH delivery", color = TextMuted.copy(alpha = 0.6f), fontSize = 10.sp)
                }
                Text(
                    formatAmount(result.totalOriginCurrency, result.originCurrency),
                    color = Cyan, fontWeight = FontWeight.ExtraBold, fontSize = 22.sp,
                    fontFamily = FontFamily.Monospace,
                )
            }
        }

        if (result.cbm > 0) {
            Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                Text("CBM", color = TextMuted, fontSize = 11.sp)
                Text("${result.cbm} m³", color = Purple, fontSize = 11.sp, fontFamily = FontFamily.Monospace)
            }
        }
        if (result.transitDays.isNotEmpty()) {
            Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                Text("Transit time", color = TextMuted, fontSize = 11.sp)
                Text(result.transitDays, color = TextPrimary, fontSize = 11.sp)
            }
        }
        Text(
            "Rates are indicative estimates. PH delivery converted from PHP.",
            color = TextMuted.copy(alpha = 0.5f), fontSize = 9.sp,
            modifier = Modifier.fillMaxWidth(), textAlign = TextAlign.Center,
        )

        Spacer(Modifier.height(4.dp))

        // ── Book This Shipment ─────────────────────────────────────────────
        // Primary CTA: navigate to booking form pre-filled with quote data.
        // Anti-fraud gate: a flagged measurement cannot be booked — the CBM that
        // priced this quote is suspect, so the hub re-measures before billing.
        val bookBlocked = state.integrity == MeasurementIntegrity.FLAGGED
        if (bookBlocked) {
            FraudBlockNote("Measurement flagged — booking is blocked until the box is re-scanned with AR or re-measured at the hub.")
        }
        Button(
            onClick = {
                onBookShipment?.invoke(
                    state.matchedSizeId,
                    state.freightMode,
                    state.originKey,
                    state.measuredL ?: 0.0,
                    state.measuredW ?: 0.0,
                    state.measuredH ?: 0.0,
                    result.totalOriginCurrency,
                    result.originCurrency,
                )
            },
            enabled = onBookShipment != null && !bookBlocked,
            modifier = Modifier.fillMaxWidth().height(54.dp),
            shape = RoundedCornerShape(14.dp),
            colors = ButtonDefaults.buttonColors(containerColor = Color.Transparent, disabledContainerColor = Color.Transparent),
            contentPadding = PaddingValues(0.dp),
        ) {
            Box(
                modifier = Modifier
                    .fillMaxSize()
                    .background(
                        if (bookBlocked) Brush.horizontalGradient(listOf(Border, Border))
                        else Brush.horizontalGradient(listOf(Cyan, Purple)),
                        RoundedCornerShape(14.dp),
                    ),
                contentAlignment = Alignment.Center,
            ) {
                Row(
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Icon(Icons.Default.LocalShipping, null, tint = if (bookBlocked) TextMuted else Canvas)
                    Text("Book This Shipment", color = if (bookBlocked) TextMuted else Canvas, fontWeight = FontWeight.Bold, fontSize = 15.sp)
                    Icon(Icons.Default.ArrowForward, null, tint = if (bookBlocked) TextMuted else Canvas, modifier = Modifier.size(16.dp))
                }
            }
        }

        // ── Share Quote ───────────────────────────────────────────────────
        // Secondary: Android share sheet — driver can send the summary to the
        // customer via WhatsApp/SMS/email without building an extra screen.
        OutlinedButton(
            onClick = {
                val size = BOX_SIZES.find { it.id == state.matchedSizeId }
                val shareText = buildString {
                    appendLine("📦 *LogisticOS Box Quote*")
                    appendLine()
                    appendLine("Box: ${size?.name ?: state.matchedSizeId}")
                    if (state.measuredL != null) {
                        appendLine("Dimensions: ${state.measuredL} × ${state.measuredW} × ${state.measuredH} cm")
                        appendLine("CBM: ${result.cbm} m³")
                    }
                    appendLine("Freight: ${state.freightMode.name.lowercase().replaceFirstChar { it.uppercase() }}")
                    appendLine("Origin: ${state.originKey}")
                    if (state.province.isNotBlank()) appendLine("Destination: ${state.province}")
                    if (result.transitDays.isNotEmpty()) appendLine("Transit: ${result.transitDays}")
                    appendLine()
                    appendLine("*Estimated Total: ${formatAmount(result.totalOriginCurrency, result.originCurrency)}*")
                    appendLine()
                    appendLine("Rates are indicative. Book via LogisticOS customer app.")
                }
                context.startActivity(
                    Intent.createChooser(
                        Intent(Intent.ACTION_SEND).apply {
                            type = "text/plain"
                            putExtra(Intent.EXTRA_TEXT, shareText)
                        },
                        "Share Quote"
                    )
                )
            },
            modifier = Modifier.fillMaxWidth().height(48.dp),
            shape = RoundedCornerShape(14.dp),
            border = BorderStroke(1.dp, Cyan.copy(alpha = 0.4f)),
            colors = ButtonDefaults.outlinedButtonColors(contentColor = Cyan),
        ) {
            Row(
                horizontalArrangement = Arrangement.spacedBy(6.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(Icons.Default.Share, null, tint = Cyan, modifier = Modifier.size(16.dp))
                Text("Share Quote", fontWeight = FontWeight.Medium, fontSize = 14.sp)
            }
        }
    }
}

@Composable
private fun SectionLabel(text: String) {
    Text(text, color = TextMuted, fontSize = 11.sp, fontWeight = FontWeight.SemiBold,
        modifier = Modifier.padding(bottom = 6.dp))
}
