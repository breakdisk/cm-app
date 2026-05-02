package io.logisticos.driver.feature.pickup.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import io.logisticos.driver.core.database.entity.TaskEntity
import io.logisticos.driver.feature.pickup.data.PickupRepository
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import javax.inject.Inject

data class PickupUiState(
    val task: TaskEntity? = null,
    val awbScanned: Boolean = false,
    val scannedAwb: String = "",
    val awbMismatch: Boolean = false,
    val photoPath: String? = null,
    val isConfirming: Boolean = false,
    val isCompleted: Boolean = false,
    val error: String? = null
) {
    val canConfirm: Boolean get() = awbScanned && !awbMismatch
}

@HiltViewModel
class PickupViewModel @Inject constructor(
    private val repo: PickupRepository
) : ViewModel() {

    private val _uiState = MutableStateFlow(PickupUiState())
    val uiState: StateFlow<PickupUiState> = _uiState.asStateFlow()

    fun load(taskId: String) {
        viewModelScope.launch {
            val task = repo.observeTask(taskId).filterNotNull().first()
            // Auto-confirm AWB when there's no scannable tracking number.
            // task.awb falls back to shipmentId (a UUID) when no real AWB was
            // assigned — drivers can't scan a UUID. Treat blank or UUID-format
            // AWBs as "no scan required" so the Confirm button is immediately
            // active. Real carrier AWBs (e.g. CM-PHL-S0012345) still require
            // the driver to scan or type them.
            val awbIsReal = task.awb.isNotBlank() && !isUuidFormat(task.awb)
            _uiState.update { it.copy(task = task, awbScanned = !awbIsReal, awbMismatch = false) }
            repo.transitionToInProgress(taskId)
        }
    }

    fun onAwbScanned(scanned: String) {
        val expected = _uiState.value.task?.awb ?: ""
        val match = scanned.trim().equals(expected.trim(), ignoreCase = true)
        _uiState.update {
            it.copy(
                scannedAwb = scanned,
                awbScanned = match,
                awbMismatch = scanned.isNotBlank() && !match
            )
        }
    }

    fun onPhotoCaptured(path: String) {
        _uiState.update { it.copy(photoPath = path) }
    }

    private fun isUuidFormat(s: String): Boolean =
        Regex("[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}", RegexOption.IGNORE_CASE).matches(s)

    fun confirmPickup(taskId: String, onDone: () -> Unit) {
        val state = _uiState.value
        if (!state.canConfirm) return
        viewModelScope.launch {
            _uiState.update { it.copy(isConfirming = true) }
            runCatching {
                repo.confirmPickup(taskId, state.photoPath)
            }.onSuccess {
                _uiState.update { it.copy(isConfirming = false, isCompleted = true) }
                onDone()
            }.onFailure { e ->
                _uiState.update { it.copy(isConfirming = false, error = e.message) }
            }
        }
    }
}
