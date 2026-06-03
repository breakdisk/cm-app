package io.logisticos.driver.feature.hub.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import io.logisticos.driver.feature.hub.data.HubRepository
import io.logisticos.driver.feature.hub.domain.HubScanType
import io.logisticos.driver.feature.scanner.domain.ScanResult
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
    /** Shipment UUID resolved from the master AWB (required by the backend). */
    val shipmentId:  String = "",
    /** Pallet UUID — required for PALLET_ASSIGN, optional otherwise. */
    val palletId:    String = "",
    /** Container UUID — required for OUTBOUND_LOAD, optional otherwise. */
    val containerId: String = "",
    /** Optional exception flag string ("missing" | "damaged" | "weight_mismatch"). */
    val exception:   String? = null,
    val isSubmitting: Boolean = false,
    val lastSubmitSuccess: Boolean? = null,   // null = not attempted yet
    val lastSubmitQueued:  Boolean  = false,  // true = submitted offline
    val error: String? = null,
) {
    /**
     * Minimum fields required to enable the Submit button.
     * Pallet / container are additionally validated in `canSubmit` when required.
     */
    val canSubmit: Boolean get() {
        if (hubId.isBlank() || pieceAwb.isBlank() || masterAwb.isBlank() || shipmentId.isBlank()) return false
        if (scanType.requiresPallet && palletId.isBlank())       return false
        if (scanType.requiresContainer && containerId.isBlank()) return false
        return !isSubmitting
    }
}

// ── ViewModel ─────────────────────────────────────────────────────────────────

@HiltViewModel
class HubScanViewModel @Inject constructor(
    private val repo: HubRepository,
) : ViewModel() {

    private val _uiState = MutableStateFlow(HubScanUiState())
    val uiState: StateFlow<HubScanUiState> = _uiState.asStateFlow()

    fun setScanType(type: HubScanType)  { _uiState.update { it.copy(scanType = type) } }
    fun setHubId(id: String)            { _uiState.update { it.copy(hubId = id) } }
    fun setMasterAwb(awb: String)       { _uiState.update { it.copy(masterAwb = awb) } }
    fun setShipmentId(id: String)       { _uiState.update { it.copy(shipmentId = id) } }
    fun setPalletId(id: String)         { _uiState.update { it.copy(palletId = id) } }
    fun setContainerId(id: String)      { _uiState.update { it.copy(containerId = id) } }
    fun setException(ex: String?)       { _uiState.update { it.copy(exception = ex) } }
    fun clearError()                    { _uiState.update { it.copy(error = null) } }

    /**
     * Called by the scanner (camera or hardware) when a barcode is decoded.
     * Captured at [deviceTimestampMillis] — the hardware clock at the physical scan moment.
     */
    fun onPieceScan(result: ScanResult, deviceTimestampMillis: Long = System.currentTimeMillis()) {
        _uiState.update {
            it.copy(
                pieceAwb        = result.rawValue.trim(),
                lastSubmitSuccess = null,
                error           = null,
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

        // Capture ISO-8601 UTC from the hardware-clock millis.
        // Must be passed in from the scan event, NOT sampled here, to honour the
        // dual-timestamp contract (device_timestamp = physical event time).
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
                        // Clear piece AWB after success so the agent can scan the next piece.
                        pieceAwb          = "",
                    )
                }
            }.onFailure { e ->
                _uiState.update {
                    it.copy(isSubmitting = false, lastSubmitSuccess = false, error = e.message)
                }
            }
        }
    }
}
