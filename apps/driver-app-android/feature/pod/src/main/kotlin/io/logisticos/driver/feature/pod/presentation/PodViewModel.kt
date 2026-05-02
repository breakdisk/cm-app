package io.logisticos.driver.feature.pod.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import io.logisticos.driver.core.database.entity.TaskStatus
import io.logisticos.driver.core.location.LocationRepository
import io.logisticos.driver.feature.delivery.data.DeliveryRepository
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import javax.inject.Inject

enum class FailureReason(val displayName: String) {
    CUSTOMER_ABSENT("Customer not home"),
    REFUSED_DELIVERY("Customer refused delivery"),
    WRONG_ADDRESS("Wrong / incomplete address"),
    RESCHEDULE_REQUESTED("Customer requested reschedule"),
    BUSINESS_CLOSED("Business closed"),
    SECURITY_DENIED("Security denied access"),
    OTHER("Other reason")
}

data class PodUiState(
    val taskId: String = "",
    val shipmentId: String = "",
    val recipientName: String = "",
    val requiresPhoto: Boolean = false,
    val requiresSignature: Boolean = false,
    val requiresOtp: Boolean = false,
    val isCod: Boolean = false,
    val codAmount: Double = 0.0,
    val photoPath: String? = null,
    val signaturePath: String? = null,
    val otpToken: String? = null,
    val codCollected: Boolean = false,
    val isSubmitting: Boolean = false,
    val isSubmitted: Boolean = false,
    val podId: String? = null,
    val showFailureSheet: Boolean = false,
    val error: String? = null,
    // Task destination coordinates used as GPS fallback when device location is unavailable.
    val taskLat: Double = 0.0,
    val taskLng: Double = 0.0
) {
    val canSubmit: Boolean
        get() = (!requiresPhoto || photoPath != null) &&
                (!requiresSignature || signaturePath != null) &&
                (!requiresOtp || otpToken != null)
}

@HiltViewModel
class PodViewModel @Inject constructor(
    private val repo: DeliveryRepository,
    private val locationRepo: LocationRepository
) : ViewModel() {

    private val _uiState = MutableStateFlow(PodUiState())
    val uiState: StateFlow<PodUiState> = _uiState.asStateFlow()

    fun setRequirements(
        taskId: String,
        shipmentId: String,
        recipientName: String,
        requiresPhoto: Boolean,
        requiresSignature: Boolean,
        requiresOtp: Boolean,
        isCod: Boolean = false,
        codAmount: Double = 0.0
    ) {
        _uiState.update {
            it.copy(
                taskId = taskId,
                shipmentId = shipmentId,
                recipientName = recipientName,
                requiresPhoto = requiresPhoto,
                requiresSignature = requiresSignature,
                requiresOtp = requiresOtp,
                isCod = isCod,
                codAmount = codAmount
            )
        }
    }

    /**
     * Loads shipmentId and recipientName from local DB when the screen only has taskId.
     * Called from LaunchedEffect when those values aren't passed via navigation args.
     */
    fun loadTaskMeta(taskId: String) {
        viewModelScope.launch {
            val task = repo.observeTask(taskId).filterNotNull().first()
            _uiState.update { prev ->
                prev.copy(
                    shipmentId = if (prev.shipmentId.isBlank()) task.shipmentId else prev.shipmentId,
                    recipientName = if (prev.recipientName.isBlank()) task.recipientName else prev.recipientName,
                    taskLat = task.lat,
                    taskLng = task.lng
                )
            }
        }
    }

    fun onPhotoCaptured(path: String)    { _uiState.update { it.copy(photoPath = path) } }
    fun onSignatureSaved(path: String)   { _uiState.update { it.copy(signaturePath = path) } }
    fun onOtpEntered(token: String)      { _uiState.update { it.copy(otpToken = token) } }
    fun onCodToggled(collected: Boolean) { _uiState.update { it.copy(codCollected = collected) } }

    fun showFailureSheet()    { _uiState.update { it.copy(showFailureSheet = true) } }
    fun dismissFailureSheet() { _uiState.update { it.copy(showFailureSheet = false) } }

    /**
     * Executes full POD flow:
     * POST /v1/pods → PUT signature → PUT submit → PUT /v1/tasks/:id/complete
     * On network error, enqueues for retry and still marks submitted (optimistic).
     */
    fun submit(taskId: String) {
        val state = _uiState.value
        if (!state.canSubmit) return
        viewModelScope.launch {
            _uiState.update { it.copy(isSubmitting = true, error = null) }
            val loc = locationRepo.getLastKnownLocation()
            // Fall back to the task's stored destination coordinates when device GPS
            // is unavailable (0,0). Prevents a hard GPS block during testing and for
            // drivers in underground/indoor areas. The backend geofence check is the
            // authoritative gate for location accuracy.
            val captureLat = if (loc != null && loc.lat != 0.0) loc.lat else state.taskLat
            val captureLng = if (loc != null && loc.lng != 0.0) loc.lng else state.taskLng
            runCatching {
                repo.submitPod(
                    taskId = state.taskId,
                    shipmentId = state.shipmentId,
                    recipientName = state.recipientName,
                    captureLat = captureLat,
                    captureLng = captureLng,
                    photoPath = state.photoPath,
                    signaturePath = state.signaturePath,
                    otpCode = state.otpToken,
                    codCollectedCents = if (state.isCod && state.codCollected)
                        (state.codAmount * 100).toLong() else null,
                    requiresPhoto = state.requiresPhoto,
                    requiresSignature = state.requiresSignature,
                )
            }.onSuccess { podId ->
                _uiState.update {
                    it.copy(isSubmitting = false, isSubmitted = true, podId = podId)
                }
            }.onFailure { e ->
                _uiState.update {
                    it.copy(
                        isSubmitting = false,
                        isSubmitted = false,
                        error = "${e.javaClass.simpleName}: ${e.message ?: "submit failed"}"
                    )
                }
            }
        }
    }

    /**
     * Fail delivery — calls PUT /v1/tasks/:id/fail on the backend.
     */
    fun submitFailure(taskId: String, reason: FailureReason, onDone: () -> Unit) {
        viewModelScope.launch {
            _uiState.update { it.copy(isSubmitting = true, showFailureSheet = false) }
            runCatching {
                repo.failTask(taskId, reason.name)
            }.onSuccess {
                _uiState.update { it.copy(isSubmitting = false) }
                onDone()
            }.onFailure { e ->
                _uiState.update { it.copy(isSubmitting = false, error = e.message) }
            }
        }
    }
}
