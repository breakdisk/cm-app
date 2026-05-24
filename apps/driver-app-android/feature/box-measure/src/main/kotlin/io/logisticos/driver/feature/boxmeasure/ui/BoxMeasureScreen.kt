package io.logisticos.driver.feature.boxmeasure.ui

import android.Manifest
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
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
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
 *
 * @param mode                 VERIFY or QUOTE
 * @param declaredSizeId       (VERIFY) size_id from the booking
 * @param declaredL/W/H        (VERIFY) declared L×W×H in cm
 * @param onDimensionsVerified (VERIFY) called with confirmed L, W, H
 * @param onBack               navigation pop
 */
@Composable
fun BoxMeasureScreen(
    mode: BoxMeasureMode = BoxMeasureMode.QUOTE,
    declaredSizeId: String? = null,
    declaredL: Double? = null,
    declaredW: Double? = null,
    declaredH: Double? = null,
    onDimensionsVerified: ((Double, Double, Double) -> Unit)? = null,
    onBack: () -> Unit = {},
    viewModel: BoxMeasureViewModel = hiltViewModel(),
) {
    val state by viewModel.uiState.collectAsState()
    val context = LocalContext.current

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
            val (l, w, h) = viewModel.activeDimensions()
            onDimensionsVerified?.invoke(l, w, h)
            onBack()
        }
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(Canvas)
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

            // ── AR camera view (full-width, 240dp tall) ────────────────────────
            // Only rendered when camera permission is granted and no session error.
            // On unsupported devices or permission denial, the error banner above
            // explains the situation and manual mode is enabled automatically.
            if (cameraGranted && state.measureError == null) {
                Box(
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(240.dp)
                        .clip(RoundedCornerShape(16.dp))
                        .border(1.dp, Border, RoundedCornerShape(16.dp)),
                ) {
                    ArCoreBoxMeasureView(
                        modifier = Modifier.fillMaxSize(),
                        viewModel = viewModel,
                    )
                    // Tap progress overlay
                    TapProgressOverlay(tapCount = state.tapCount)
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
                    onConfirm   = viewModel::confirmDimensions,
                )
            }

            // ── QUOTE mode: quote inputs + result ─────────────────────────────
            if (mode == BoxMeasureMode.QUOTE) {
                QuoteInputSection(state = state, viewModel = viewModel)
            }

            Spacer(Modifier.height(32.dp))
        }
    }
}

// ── Sub-composables ────────────────────────────────────────────────────────────

@Composable
private fun TapProgressOverlay(tapCount: Int) {
    val steps = listOf("Tap length start", "Tap length end", "Tap width", "Tap height")
    Column(
        modifier = Modifier
            .padding(12.dp)
            .background(Color(0xBB050810), RoundedCornerShape(10.dp))
            .padding(horizontal = 12.dp, vertical = 8.dp),
    ) {
        Text(
            text = if (tapCount < 4) "Step ${tapCount + 1} / 4 — ${steps[tapCount]}"
                   else "Measurement complete ✓",
            color = Cyan, fontSize = 12.sp, fontWeight = FontWeight.Bold,
            fontFamily = FontFamily.Monospace,
        )
        if (tapCount < 4) {
            Row(modifier = Modifier.padding(top = 6.dp), horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                repeat(4) { i ->
                    Box(
                        modifier = Modifier
                            .size(width = if (i < tapCount) 24.dp else if (i == tapCount) 32.dp else 12.dp, height = 4.dp)
                            .clip(RoundedCornerShape(2.dp))
                            .background(if (i <= tapCount) Cyan else Border),
                    )
                }
            }
        }
    }
}

@Composable
private fun MeasurementCard(
    measuredL: Double, measuredW: Double, measuredH: Double, confidence: Double,
    declaredL: Double?, declaredW: Double?, declaredH: Double?,
) {
    val hasDeclared = declaredL != null
    val cbm = computeCbm(measuredL, measuredW, measuredH)

    Surface(
        color = Glass,
        shape = RoundedCornerShape(16.dp),
        border = BorderStroke(1.dp, Cyan.copy(alpha = 0.25f)),
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
private fun VerifyConfirmSection(hasResult: Boolean, onConfirm: () -> Unit) {
    if (!hasResult) {
        Text(
            "Complete the 4-tap AR measurement or enter dimensions manually to confirm.",
            color = TextMuted, fontSize = 12.sp, textAlign = TextAlign.Center,
            modifier = Modifier.fillMaxWidth().padding(vertical = 8.dp),
        )
    } else {
        Button(
            onClick = onConfirm,
            modifier = Modifier.fillMaxWidth().height(52.dp),
            shape = RoundedCornerShape(14.dp),
            colors = ButtonDefaults.buttonColors(containerColor = Color.Transparent),
            contentPadding = PaddingValues(0.dp),
        ) {
            Box(
                modifier = Modifier
                    .fillMaxSize()
                    .background(Brush.horizontalGradient(listOf(Cyan, Purple)), RoundedCornerShape(14.dp)),
                contentAlignment = Alignment.Center,
            ) {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                    Icon(Icons.Default.Check, null, tint = Canvas)
                    Text("Confirm Dimensions", color = Canvas, fontWeight = FontWeight.Bold, fontSize = 15.sp)
                }
            }
        }
    }
}

@OptIn(androidx.compose.material3.ExperimentalMaterial3Api::class)
@Composable
private fun QuoteInputSection(state: BoxMeasureUiState, viewModel: BoxMeasureViewModel) {

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
    }
}

@Composable
private fun SectionLabel(text: String) {
    Text(text, color = TextMuted, fontSize = 11.sp, fontWeight = FontWeight.SemiBold,
        modifier = Modifier.padding(bottom = 6.dp))
}
