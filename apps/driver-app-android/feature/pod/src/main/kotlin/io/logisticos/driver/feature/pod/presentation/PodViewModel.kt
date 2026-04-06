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

data class PodUiState(
    val taskId: String = "",
    val requiresPhoto: Boolean = false,
    val requiresSignature: Boolean = false,
    val requiresOtp: Boolean = false,
    val photoPath: String? = null,
    val signaturePath: String? = null,
    val otpToken: String? = null,
    val otpSent: Boolean = false,
    val isSubmitting: Boolean = false,
    val isSubmitted: Boolean = false,
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
        requiresOtp: Boolean
    ) {
        _uiState.update {
            it.copy(
                taskId = taskId,
                requiresPhoto = requiresPhoto,
                requiresSignature = requiresSignature,
                requiresOtp = requiresOtp
            )
        }
    }

    fun onPhotoCaptured(path: String) { _uiState.update { it.copy(photoPath = path) } }
    fun onSignatureSaved(path: String) { _uiState.update { it.copy(signaturePath = path) } }
    fun onOtpEntered(token: String) { _uiState.update { it.copy(otpToken = token) } }

    fun submit() {
        val state = _uiState.value
        if (!state.canSubmit) return
        viewModelScope.launch {
            _uiState.update { it.copy(isSubmitting = true) }
            runCatching {
                repo.savePod(state.taskId, state.photoPath, state.signaturePath, state.otpToken)
                repo.transitionTask(state.taskId, TaskStatus.COMPLETED)
            }.onSuccess {
                _uiState.update { it.copy(isSubmitting = false, isSubmitted = true) }
            }.onFailure { e ->
                _uiState.update { it.copy(isSubmitting = false, error = e.message) }
            }
        }
    }
}
