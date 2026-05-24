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

data class BoxMeasureUiState(
    // ── AR measurement ──────────────────────────────────────────────────────────
    val arSessionReady: Boolean = false,
    val tapCount: Int = 0,           // 0–4 taps to measure L, W, H
    val measuredL: Double? = null,
    val measuredW: Double? = null,
    val measuredH: Double? = null,
    val arConfidence: Double = 0.0,
    val measureError: String? = null,

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
        _uiState.update { it.copy(
            measuredL      = l,
            measuredW      = w,
            measuredH      = h,
            arConfidence   = confidence,
            matchedSizeId  = matched?.id ?: it.matchedSizeId,
            tapCount       = 4,
            measureError   = null,
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
        _uiState.update { it.copy(
            measuredL     = l,
            measuredW     = w,
            measuredH     = h,
            matchedSizeId = matched?.id ?: it.matchedSizeId,
            manualMode    = false,
        )}
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
}
