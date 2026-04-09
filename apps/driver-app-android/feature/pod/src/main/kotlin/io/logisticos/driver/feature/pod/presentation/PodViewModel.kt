package io.logisticos.driver.feature.pod.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import io.logisticos.driver.core.database.entity.TaskStatus
import io.logisticos.driver.feature.delivery.data.DeliveryRepository
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
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
    val showFailureSheet: Boolean = false,
    val error: String? = null
) {
    val canSubmit: Boolean
        get() = (!requiresPhoto || photoPath != null) &&
                (!requiresSignature || signaturePath != null) &&
                (!requiresOtp || otpToken != null)
}

@HiltViewModel
class PodViewModel @Inject constructor(
    private val repo: DeliveryRepository
) : ViewModel() {

    private val _uiState = MutableStateFlow(PodUiState())
    val uiState: StateFlow<PodUiState> = _uiState.asStateFlow()

    fun setRequirements(
        taskId: String,
        requiresPhoto: Boolean,
        requiresSignature: Boolean,
        requiresOtp: Boolean,
        isCod: Boolean = false,
        codAmount: Double = 0.0
    ) {
        _uiState.update {
            it.copy(
                taskId = taskId,
                requiresPhoto = requiresPhoto,
                requiresSignature = requiresSignature,
                requiresOtp = requiresOtp,
                isCod = isCod,
                codAmount = codAmount
            )
        }
    }

    fun onPhotoCaptured(path: String)    { _uiState.update { it.copy(photoPath = path) } }
    fun onSignatureSaved(path: String)   { _uiState.update { it.copy(signaturePath = path) } }
    fun onOtpEntered(token: String)      { _uiState.update { it.copy(otpToken = token) } }
    fun onCodToggled(collected: Boolean) { _uiState.update { it.copy(codCollected = collected) } }

    fun showFailureSheet()    { _uiState.update { it.copy(showFailureSheet = true) } }
    fun dismissFailureSheet() { _uiState.update { it.copy(showFailureSheet = false) } }

    fun submit(taskId: String) {
        val state = _uiState.value
        if (!state.canSubmit) return
        viewModelScope.launch {
            _uiState.update { it.copy(isSubmitting = true) }
            runCatching {
                repo.savePod(state.taskId, state.photoPath, state.signaturePath, state.otpToken)
                repo.transitionTask(state.taskId, TaskStatus.COMPLETED)
                if (state.isCod && state.codCollected) {
                    val shiftId = repo.getActiveShiftId()
                    if (shiftId != null) repo.confirmCod(shiftId, state.taskId, state.codAmount)
                }
            }.onSuccess {
                _uiState.update { it.copy(isSubmitting = false, isSubmitted = true) }
            }.onFailure { e ->
                _uiState.update { it.copy(isSubmitting = false, error = e.message) }
            }
        }
    }

    fun submitFailure(taskId: String, reason: FailureReason, onDone: () -> Unit) {
        viewModelScope.launch {
            _uiState.update { it.copy(isSubmitting = true, showFailureSheet = false) }
            runCatching {
                repo.transitionTask(taskId, TaskStatus.FAILED)
                repo.saveFailureReason(taskId, reason.name)
            }.onSuccess {
                _uiState.update { it.copy(isSubmitting = false) }
                onDone()
            }.onFailure { e ->
                _uiState.update { it.copy(isSubmitting = false, error = e.message) }
            }
        }
    }
}
