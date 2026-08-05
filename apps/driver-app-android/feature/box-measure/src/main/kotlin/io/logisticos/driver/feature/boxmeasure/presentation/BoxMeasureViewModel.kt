package io.logisticos.driver.feature.boxmeasure.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import io.logisticos.driver.feature.boxmeasure.data.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import javax.inject.Inject

/**
 * Modes:
 *   VERIFY — launched from PickupScreen; declaredDimensions populated from shipment.
 *            Confirms or overrides dimensions before POP submission.
 *   QUOTE  — standalone from HomeScreen; no declared dimensions pre-filled.
 */
enum class BoxMeasureMode { VERIFY, QUOTE }

/**
 * Integrity classification for a captured measurement — the anti-fraud surface.
 *
 * Freight is priced on CBM, so an under-stated box directly under-bills the
 * shipment. Every measurement is scored before it is allowed to feed a quote or
 * a POP confirmation:
 *
 *   VERIFIED — high-confidence AR scan, or a manual edit within tolerance of one.
 *   REVIEW   — usable but unverified: low AR confidence, or manual-only entry.
 *   FLAGGED  — manual entry materially smaller than the trusted AR scan. Booking
 *              and POP confirmation are blocked; the hub re-measures before billing.
 */
enum class MeasurementIntegrity { PENDING, VERIFIED, REVIEW, FLAGGED }

/** The three measured cuboid axes — drives the AR dimension chip colours. */
enum class DimAxis { LENGTH, WIDTH, HEIGHT }

/**
 * A projected AR dimension label: the screen-pixel position of a cuboid edge
 * midpoint (computed by the renderer each frame) plus its measured length in cm.
 * Rendered as a floating chip over the camera, value shown in inches.
 */
data class DimLabel(
    val axis: DimAxis,
    val xPx: Float,
    val yPx: Float,
    val cm: Double,
)

/**
 * AR-measured dimensioning snapshot handed to the Proof-of-Pickup flow. Serves the
 * POP triple purpose: anti-fraud (integrity + dimensions), size audit (cbm /
 * volumetric weight), and box count (quantity). [integrity] is a
 * [MeasurementIntegrity] name.
 */
data class PopDimensioning(
    val lengthCm: Double,
    val widthCm: Double,
    val heightCm: Double,
    val cbm: Double,
    val volumetricWeightKg: Double,
    val quantity: Int,
    val integrity: String,
)

/** Below this ARCore tracking confidence a scan is downgraded to REVIEW. */
private const val AR_CONFIDENCE_FLOOR = 0.80

/** A manual edit shrinking AR-scanned volume by more than this share is FLAGGED. */
private const val MANUAL_UNDERCUT_TOLERANCE = 0.10

data class BoxMeasureUiState(
    // ── AR measurement ──────────────────────────────────────────────────────────
    val arSessionReady: Boolean = false,
    val tapCount: Int = 0,           // 0–4 taps to measure L, W, H
    val measuredL: Double? = null,
    val measuredW: Double? = null,
    val measuredH: Double? = null,
    val arConfidence: Double = 0.0,
    val measureError: String? = null,

    // Live edge-distance under the centre reticle: distance in **cm** from the last
    // placed anchor to the current camera aim point.  Null when no anchor is placed yet
    // or when the reticle misses a plane.  Drives the LiveMeasureReticle chip colour
    // and value (cyan = LENGTH, pink = WIDTH, green = HEIGHT).
    val liveDistanceCm: Double? = null,

    // Projected AR dimension chips (length / width / height edge midpoints).
    val dimLabels: List<DimLabel> = emptyList(),

    // ── Measurement integrity / anti-fraud ──────────────────────────────────────
    val integrity: MeasurementIntegrity = MeasurementIntegrity.PENDING,
    val integrityReason: String? = null,
    val manualOverrideUsed: Boolean = false,
    // Trusted AR-scan snapshot, retained even after a manual override, so the
    // deviation between the scan and the edited values stays auditable.
    val arScanL: Double? = null,
    val arScanW: Double? = null,
    val arScanH: Double? = null,

    // ── Manual override inputs ──────────────────────────────────────────────────
    val manualMode: Boolean = false,
    val manualL: String = "",
    val manualW: String = "",
    val manualH: String = "",

    // ── Declared (for VERIFY mode) ──────────────────────────────────────────────
    val declaredSizeId: String? = null,
    val declaredL: Double? = null,
    val declaredW: Double? = null,
    val declaredH: Double? = null,

    // ── Matched standard size ────────────────────────────────────────────────────
    val matchedSizeId: String = "jumbo",
    val qty: Int = 1,

    // ── Quote inputs ────────────────────────────────────────────────────────────
    val freightMode: FreightMode = FreightMode.SEA,
    val originKey: String = SEA_CARGO.first().origin,
    val weightKg: Double = 20.0,
    val province: String = "",

    // ── Quote result ─────────────────────────────────────────────────────────────
    val quoteResult: QuoteResult? = null,
    val isCalculating: Boolean = false,

    // ── Confirmation (VERIFY mode only) ─────────────────────────────────────────
    val dimensionConfirmed: Boolean = false,

    // Bumped by resetMeasurement() to signal the AR renderer to drop its tap points.
    val resetToken: Int = 0,
)

@HiltViewModel
class BoxMeasureViewModel @Inject constructor() : ViewModel() {

    private val _uiState = MutableStateFlow(BoxMeasureUiState())
    val uiState: StateFlow<BoxMeasureUiState> = _uiState.asStateFlow()

    // ── Init ──────────────────────────────────────────────────────────────────────

    fun initVerifyMode(
        declaredSizeId: String?,
        declaredL: Double?,
        declaredW: Double?,
        declaredH: Double?,
    ) {
        _uiState.update { it.copy(
            declaredSizeId = declaredSizeId,
            declaredL      = declaredL,
            declaredW      = declaredW,
            declaredH      = declaredH,
            matchedSizeId  = declaredSizeId ?: "jumbo",
        )}
    }

    // ── AR session events ─────────────────────────────────────────────────────────

    fun onArSessionReady() = _uiState.update { it.copy(arSessionReady = true) }

    /**
     * Live edge distance (cm) from the last placed anchor to the reticle aim point,
     * posted by the renderer ~6 Hz.  Null when no anchor is placed or when the centre
     * hit-test misses a tracked plane.
     */
    fun onLiveDistance(cm: Double?) =
        _uiState.update { it.copy(liveDistanceCm = cm) }

    /** Projected AR dimension chip positions, throttled by the renderer. */
    fun onDimLabels(labels: List<DimLabel>) =
        _uiState.update { it.copy(dimLabels = labels) }

    /**
     * Clears the current measurement so the driver can re-scan. Drops the captured
     * dimensions, the integrity score, and the trusted AR-scan snapshot, and bumps
     * [BoxMeasureUiState.resetToken] — the AR view observes the token and clears the
     * renderer's world-space tap points so the next tap starts a fresh box.
     */
    fun resetMeasurement() = _uiState.update { it.copy(
        tapCount           = 0,
        measuredL          = null,
        measuredW          = null,
        measuredH          = null,
        arScanL            = null,
        arScanW            = null,
        arScanH            = null,
        arConfidence       = 0.0,
        integrity          = MeasurementIntegrity.PENDING,
        integrityReason    = null,
        manualOverrideUsed = false,
        liveDistanceCm     = null,
        dimLabels          = emptyList(),
        resetToken         = it.resetToken + 1,
    )}

    fun onArSessionError(msg: String) = _uiState.update { it.copy(measureError = msg, arSessionReady = false) }

    /**
     * Called by ArCoreBoxMeasureView when the user taps a corner.
     * Points arrive in order: [length_start, length_end, width_end, height_top]
     */
    fun onMeasurementPoint(index: Int, worldX: Float, worldY: Float, worldZ: Float) {
        _uiState.update { state ->
            when (index) {
                1 -> state.copy(tapCount = 1)
                2 -> {
                    // L = dist between world points 0 and 1 — stored in VM after ARCore callback
                    state.copy(tapCount = 2)
                }
                3 -> state.copy(tapCount = 3)
                4 -> state.copy(tapCount = 4)
                else -> state
            }
        }
    }

    fun onMeasurementComplete(l: Double, w: Double, h: Double, confidence: Double) {
        val matched = matchToStandardSize(l, w, h)
        // A measurement outside every standard box's tolerance is a real outcome,
        // not a match failure to paper over. Flagging it for review keeps the POP
        // dimensioning record honest — carrying the previous size forward would
        // record a non-standard parcel as a verified standard one.
        val nonStandard = matched == null
        val integrity = when {
            nonStandard -> MeasurementIntegrity.REVIEW
            confidence >= AR_CONFIDENCE_FLOOR -> MeasurementIntegrity.VERIFIED
            else -> MeasurementIntegrity.REVIEW
        }
        val reason = when {
            nonStandard ->
                "Measured ${"%.0f".format(l)}x${"%.0f".format(w)}x${"%.0f".format(h)} cm does not match a standard box size — confirm the dimensions or handle as non-standard."
            confidence < AR_CONFIDENCE_FLOOR ->
                "Low AR tracking confidence (${(confidence * 100).toInt()}%) — re-scan in brighter light against a flatter surface, or enter manually."
            else -> null
        }
        _uiState.update { it.copy(
            measuredL          = l,
            measuredW          = w,
            measuredH          = h,
            arScanL            = l,
            arScanW            = w,
            arScanH            = h,
            arConfidence       = confidence,
            // Retained on a non-match because matchedSizeId is non-nullable and
            // feeds BoxSizeSelector and computeQuote, both of which require a
            // value. The stale-size risk is covered by the REVIEW integrity flag
            // and its reason above, so the condition is no longer silent — but
            // modelling "no standard size" properly needs this to become
            // nullable, which is a wider change than this fix.
            matchedSizeId      = matched?.id ?: it.matchedSizeId,
            tapCount           = 4,
            measureError       = null,
            manualOverrideUsed = false,
            integrity          = integrity,
            integrityReason    = reason,
        )}
    }

    // ── Manual input ──────────────────────────────────────────────────────────────

    fun setManualMode(enabled: Boolean) = _uiState.update { it.copy(manualMode = enabled) }
    fun setManualL(v: String) = _uiState.update { it.copy(manualL = v) }
    fun setManualW(v: String) = _uiState.update { it.copy(manualW = v) }
    fun setManualH(v: String) = _uiState.update { it.copy(manualH = v) }

    fun applyManualDimensions() {
        val l = _uiState.value.manualL.toDoubleOrNull() ?: return
        val w = _uiState.value.manualW.toDoubleOrNull() ?: return
        val h = _uiState.value.manualH.toDoubleOrNull() ?: return
        val matched = matchToStandardSize(l, w, h)
        val (integrity, reason) = assessManualOverride(_uiState.value, l, w, h)
        _uiState.update { it.copy(
            measuredL          = l,
            measuredW          = w,
            measuredH          = h,
            matchedSizeId      = matched?.id ?: it.matchedSizeId,
            manualMode         = false,
            manualOverrideUsed = true,
            integrity          = integrity,
            integrityReason    = reason,
        )}
    }

    /**
     * Scores a manual entry against the trusted AR scan (if any).
     *
     * Manual entry is the primary fraud vector: a driver/agent can type a smaller
     * box than the one in hand to lower the CBM and the quoted freight. When an AR
     * scan exists we compare volumes — shrinking it past [MANUAL_UNDERCUT_TOLERANCE]
     * is FLAGGED (blocks booking/POP). With no AR baseline the entry is allowed but
     * only ever REVIEW — it is auditable but never silently trusted.
     */
    private fun assessManualOverride(
        s: BoxMeasureUiState, l: Double, w: Double, h: Double,
    ): Pair<MeasurementIntegrity, String?> {
        val scanL = s.arScanL; val scanW = s.arScanW; val scanH = s.arScanH
        if (scanL == null || scanW == null || scanH == null) {
            return MeasurementIntegrity.REVIEW to
                "Entered manually without an AR scan — unverified, and subject to re-measurement at the hub."
        }
        val scanVol   = scanL * scanW * scanH
        val manualVol = l * w * h
        if (scanVol <= 0.0) return MeasurementIntegrity.REVIEW to null
        val undercut = (scanVol - manualVol) / scanVol
        return if (undercut > MANUAL_UNDERCUT_TOLERANCE) {
            val pct = (undercut * 100).toInt()
            MeasurementIntegrity.FLAGGED to
                "Manual entry is $pct% smaller than the AR scan " +
                "(${"%.0f".format(scanL)}×${"%.0f".format(scanW)}×${"%.0f".format(scanH)} cm). " +
                "Flagged for review — the hub will re-measure before billing."
        } else {
            MeasurementIntegrity.VERIFIED to null
        }
    }

    // ── Quote inputs ──────────────────────────────────────────────────────────────

    fun setFreightMode(mode: FreightMode) = _uiState.update {
        it.copy(
            freightMode = mode,
            originKey   = if (mode == FreightMode.SEA) SEA_CARGO.first().origin else AIR_ZONES.first().zoneName,
        )
    }
    fun setOriginKey(key: String)   = _uiState.update { it.copy(originKey = key) }
    fun setWeightKg(v: Double)      = _uiState.update { it.copy(weightKg = v) }
    fun setProvince(v: String)      = _uiState.update { it.copy(province = v) }
    fun setMatchedSizeId(id: String)= _uiState.update { it.copy(matchedSizeId = id) }
    fun setQty(q: Int)              = _uiState.update { it.copy(qty = q.coerceIn(1, 20)) }

    // ── Quote calculation ─────────────────────────────────────────────────────────

    fun calculateQuote() {
        val s = _uiState.value
        val (l, w, h) = resolveActiveDimensions(s)
        _uiState.update { it.copy(isCalculating = true) }
        viewModelScope.launch {
            val result = computeQuote(
                mode      = s.freightMode,
                originKey = s.originKey,
                sizeId    = s.matchedSizeId,
                qty       = s.qty,
                weightKg  = s.weightKg,
                dimsCm    = Triple(l, w, h),
                province  = s.province,
            )
            _uiState.update { it.copy(quoteResult = result, isCalculating = false) }
        }
    }

    fun clearQuote() = _uiState.update { it.copy(quoteResult = null) }

    // ── Verify mode confirm ───────────────────────────────────────────────────────

    fun confirmDimensions() = _uiState.update { it.copy(dimensionConfirmed = true) }

    // ── Helpers ────────────────────────────────────────────────────────────────────

    private fun resolveActiveDimensions(s: BoxMeasureUiState): Triple<Double, Double, Double> {
        if (s.measuredL != null) return Triple(s.measuredL, s.measuredW ?: 0.0, s.measuredH ?: 0.0)
        val size = BOX_SIZES.find { it.id == s.matchedSizeId }
        return if (size != null) {
            val (l, w, h) = size.dimsCm
            Triple(l.toDouble(), w.toDouble(), h.toDouble())
        } else Triple(0.0, 0.0, 0.0)
    }

    fun activeDimensions(): Triple<Double, Double, Double> = resolveActiveDimensions(_uiState.value)

    /** Full dimensioning snapshot for the POP flow (anti-fraud / audit / quantity). */
    fun popDimensioning(): PopDimensioning {
        val s = _uiState.value
        val (l, w, h) = resolveActiveDimensions(s)
        return PopDimensioning(
            lengthCm           = l,
            widthCm            = w,
            heightCm           = h,
            cbm                = computeCbm(l, w, h),
            volumetricWeightKg = computeVolumetricWeight(l, w, h),
            quantity           = s.qty,
            integrity          = s.integrity.name,
        )
    }
}
