package io.logisticos.driver.feature.delivery.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import io.logisticos.driver.core.database.entity.TaskEntity
import io.logisticos.driver.core.database.entity.TaskStatus
import io.logisticos.driver.feature.delivery.data.DeliveryRepository
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import javax.inject.Inject

data class ArrivalUiState(
    val task: TaskEntity? = null,
    val isTransitioning: Boolean = false,
    val isSubmittingPod: Boolean = false,
    val podSubmitted: Boolean = false,
    val podId: String? = null,
    val error: String? = null
)

@HiltViewModel
class ArrivalViewModel @Inject constructor(
    private val repo: DeliveryRepository
) : ViewModel() {

    private val _uiState = MutableStateFlow(ArrivalUiState())
    val uiState: StateFlow<ArrivalUiState> = _uiState.asStateFlow()

    fun load(taskId: String) {
        viewModelScope.launch {
            repo.observeTask(taskId)
                .filterNotNull()
                .first()
                .let { task -> _uiState.update { it.copy(task = task) } }
        }
    }

    /**
     * Transitions task → IN_PROGRESS then invokes [onReady].
     * Calls backend PUT /v1/tasks/:id/start; falls back to sync queue offline.
     */
    fun startTask(taskId: String, onReady: () -> Unit) {
        viewModelScope.launch {
            _uiState.update { it.copy(isTransitioning = true) }
            runCatching {
                repo.transitionTask(taskId, TaskStatus.IN_PROGRESS)
            }.onSuccess {
                _uiState.update { it.copy(isTransitioning = false) }
                onReady()
            }.onFailure { e ->
                _uiState.update { it.copy(isTransitioning = false, error = e.message) }
            }
        }
    }

    /**
     * Executes the full POD submission flow:
     * POST /v1/pods → PUT /v1/pods/:id/signature → PUT /v1/pods/:id/submit →
     * PUT /v1/tasks/:id/complete
     *
     * On success calls [onDone] with the pod_id.
     * On network failure, enqueues for retry and still calls [onDone] (optimistic).
     */
    fun submitPod(
        taskId: String,
        shipmentId: String,
        recipientName: String,
        captureLat: Double,
        captureLng: Double,
        photoPath: String? = null,
        signaturePath: String? = null,
        otpCode: String? = null,
        codCollectedCents: Long? = null,
        onDone: (podId: String?) -> Unit
    ) {
        viewModelScope.launch {
            _uiState.update { it.copy(isSubmittingPod = true, error = null) }
            val podId = repo.submitPod(
                taskId = taskId,
                shipmentId = shipmentId,
                recipientName = recipientName,
                captureLat = captureLat,
                captureLng = captureLng,
                photoPath = photoPath,
                signaturePath = signaturePath,
                otpCode = otpCode,
                codCollectedCents = codCollectedCents
            )
            _uiState.update { it.copy(isSubmittingPod = false, podSubmitted = true, podId = podId) }
            onDone(podId)
        }
    }

    /**
     * Fail delivery — calls backend PUT /v1/tasks/:id/fail.
     */
    fun failTask(taskId: String, reason: String, onDone: () -> Unit) {
        viewModelScope.launch {
            _uiState.update { it.copy(isTransitioning = true) }
            runCatching {
                repo.failTask(taskId, reason)
            }.onSuccess {
                _uiState.update { it.copy(isTransitioning = false) }
                onDone()
            }.onFailure { e ->
                _uiState.update { it.copy(isTransitioning = false, error = e.message) }
            }
        }
    }

    fun clearError() = _uiState.update { it.copy(error = null) }
}
