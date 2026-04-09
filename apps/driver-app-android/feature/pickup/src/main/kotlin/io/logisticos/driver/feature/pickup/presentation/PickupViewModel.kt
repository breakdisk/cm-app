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
            _uiState.update { it.copy(task = task) }
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
                awbMismatch = !match
            )
        }
    }

    fun onPhotoCaptured(path: String) {
        _uiState.update { it.copy(photoPath = path) }
    }

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
