package io.logisticos.driver.feature.hub.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import io.logisticos.driver.feature.hub.data.HubRepository
import io.logisticos.driver.feature.hub.domain.HubScanType
import io.logisticos.driver.feature.scanner.domain.ScanResult
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import javax.inject.Inject

// ── UI State ──────────────────────────────────────────────────────────────────

data class HubScanUiState(
    /** Scan type selected by the hub agent. */
    val scanType:    HubScanType = HubScanType.INBOUND_RECEIVE,
    /** Hub UUID — provided by the hub agent at session start or from auth claims. */
    val hubId:       String = "",
    /** Most recent piece AWB scanned by the camera / hardware scanner. */
    val pieceAwb:    String = "",
    /** Master AWB entered or pre-filled from the task context. */
    val masterAwb:   String = "",
    /** Shipment UUID — auto-resolved from masterAwb or manually entered. */
    val shipmentId:  String = "",
    /** Pallet UUID — required for PALLET_ASSIGN, optional otherwise. */
    val palletId:    String = "",
    /** Container UUID — required for OUTBOUND_LOAD and CONTAINER_DECONSOLIDATE, optional otherwise. */
    val containerId: String = "",
    /** Optional exception flag string ("missing" | "damaged" | "weight_mismatch"). */
    val exception:   String? = null,
    val isSubmitting: Boolean = false,
    val lastSubmitSuccess: Boolean? = null,   // null = not attempted yet
    val lastSubmitQueued:  Boolean  = false,  // true = submitted offline
    val error: String? = null,
    /** True while a backend AWB → shipment-UUID lookup is in flight. */
    val isResolvingShipment:  Boolean = false,
    /** True when the lookup returned 404 — agent must type the UUID manually. */
    val shipmentResolveFailed: Boolean = false,
) {
    /**
     * Minimum fields required to enable the Submit button.
     */
    val canSubmit: Boolean get() {
        if (hubId.isBlank() || pieceAwb.isBlank() || masterAwb.isBlank() || shipmentId.isBlank()) return false
        if (scanType.requiresPallet && palletId.isBlank())                 return false
        if (scanType.requiresContainer && containerId.isBlank())           return false
        if (scanType == HubScanType.EXCEPTION_FLAG && exception.isNullOrBlank()) return false
        if (isSubmitting || isResolvingShipment)                           return false
        return true
    }
}

// ── ViewModel ─────────────────────────────────────────────────────────────────

@HiltViewModel
class HubScanViewModel @Inject constructor(
    private val repo: HubRepository,
) : ViewModel() {

    private val _uiState = MutableStateFlow(HubScanUiState())
    val uiState: StateFlow<HubScanUiState> = _uiState.asStateFlow()

    /** Tracks the active AWB-resolve coroutine so it can be cancelled on new input. */
    private var resolveJob: Job? = null

    fun setScanType(type: HubScanType)  { _uiState.update { it.copy(scanType = type) } }
    fun setHubId(id: String)            { _uiState.update { it.copy(hubId = id) } }
    fun setShipmentId(id: String)       { _uiState.update { it.copy(shipmentId = id) } }
    fun setPalletId(id: String)         { _uiState.update { it.copy(palletId = id) } }
    fun setContainerId(id: String)      { _uiState.update { it.copy(containerId = id) } }
    fun setException(ex: String?)       { _uiState.update { it.copy(exception = ex) } }
    fun clearError()                    { _uiState.update { it.copy(error = null) } }

    /**
     * Updates masterAwb. When the value matches the canonical AWB pattern
     * `CM-[A-Z]{3}-[A-Z][0-9]{7}`, a backend lookup is launched automatically
     * to populate [HubScanUiState.shipmentId]. Any in-flight resolve is cancelled
     * first so rapid typing / camera rescans don't stack coroutines.
     */
    fun setMasterAwb(awb: String) {
        _uiState.update { it.copy(masterAwb = awb) }
        resolveJob?.cancel()
        when {
            awb.isBlank() -> _uiState.update {
                it.copy(
                    shipmentId             = "",
                    isResolvingShipment    = false,
                    shipmentResolveFailed  = false,
                )
            }
            AWB_PATTERN.matches(awb) -> {
                resolveJob = viewModelScope.launch { resolveShipmentId(awb) }
            }
        }
    }

    /** Resolves [awb] to a shipment UUID via the backend and writes the result to state. */
    private suspend fun resolveShipmentId(awb: String) {
        _uiState.update { it.copy(isResolvingShipment = true, shipmentResolveFailed = false) }
        runCatching { repo.lookupShipmentByAwb(awb) }
            .onSuccess { id ->
                if (id != null) {
                    _uiState.update { it.copy(shipmentId = id, isResolvingShipment = false) }
                } else {
                    _uiState.update {
                        it.copy(isResolvingShipment = false, shipmentResolveFailed = true)
                    }
                }
            }
            .onFailure { e ->
                _uiState.update { it.copy(isResolvingShipment = false, error = e.message) }
            }
    }

    /**
     * Called by the scanner (camera or hardware) when a barcode is decoded.
     * Captured at [deviceTimestampMillis] — the hardware clock at the physical scan moment.
     */
    fun onPieceScan(result: ScanResult, deviceTimestampMillis: Long = System.currentTimeMillis()) {
        _uiState.update {
            it.copy(
                pieceAwb          = result.rawValue.trim(),
                lastSubmitSuccess = null,
                error             = null,
            )
        }
        // Auto-submit when all required context is already filled in.
        if (_uiState.value.canSubmit) {
            submitScan(deviceTimestampMillis)
        }
    }

    /**
     * Manual submission — called by the "Submit" button after the agent fills
     * in all context fields and scans the piece AWB.
     */
    fun submitScan(deviceTimestampMillis: Long = System.currentTimeMillis()) {
        val state = _uiState.value
        if (!state.canSubmit) return

        val deviceTimestamp = HubRepository.isoFromMillis(deviceTimestampMillis)

        viewModelScope.launch {
            _uiState.update { it.copy(isSubmitting = true, error = null) }
            runCatching {
                repo.recordScan(
                    hubId           = state.hubId,
                    pieceAwb        = state.pieceAwb,
                    masterAwb       = state.masterAwb,
                    shipmentId      = state.shipmentId,
                    scanType        = state.scanType,
                    deviceTimestamp = deviceTimestamp,
                    palletId        = state.palletId.ifBlank { null },
                    containerId     = state.containerId.ifBlank { null },
                    exception       = state.exception,
                )
            }.onSuccess { online ->
                _uiState.update {
                    it.copy(
                        isSubmitting      = false,
                        lastSubmitSuccess = true,
                        lastSubmitQueued  = !online,
                        pieceAwb          = "",
                    )
                }
            }.onFailure { e ->
                _uiState.update {
                    it.copy(
                        isSubmitting      = false,
                        lastSubmitSuccess = false,
                        error             = e.message,
                        pieceAwb          = "",
                    )
                }
            }
        }
    }

    companion object {
        /** Canonical AWB pattern: CM-{TTT}-{S}{7 digits}, e.g. CM-PHL-S0012345 */
        private val AWB_PATTERN = Regex("^CM-[A-Z]{3}-[A-Z]\\d{7}$")
    }
}
