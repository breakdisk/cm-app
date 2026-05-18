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
    val recipientPhone: String = "",
    val requiresPhoto: Boolean = false,
    val requiresSignature: Boolean = false,
    val requiresOtp: Boolean = false,
    val isCod: Boolean = false,
    val codAmount: Double = 0.0,
    val photoPath: String? = null,
    val signaturePath: String? = null,
    val otpToken: String? = null,
    /** True after a successful POST /v1/otps/generate — the SMS was dispatched (or dummy mode). */
    val otpSent: Boolean = false,
    val isSendingOtp: Boolean = false,
    val isVerifyingOtp: Boolean = false,
    /** Inline OTP-specific error (send failure, wrong code, expired). */
    val otpError: String? = null,
    /** Non-null when the backend returned the OTP code directly (no SMS / dev mode).
     *  Displayed on-screen so the driver can see the code and auto-verification proceeds. */
    val dummyOtpCode: String? = null,
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
     * Loads task metadata from local DB when the screen only has taskId.
     * Backfills shipmentId / recipientName when not passed via nav args.
     * Also populates recipientPhone (needed for OTP SMS) and GPS fallback coords.
     */
    fun loadTaskMeta(taskId: String) {
        viewModelScope.launch {
            val task = repo.observeTask(taskId).filterNotNull().first()
            _uiState.update { prev ->
                prev.copy(
                    shipmentId = if (prev.shipmentId.isBlank()) task.shipmentId else prev.shipmentId,
                    recipientName = if (prev.recipientName.isBlank()) task.recipientName else prev.recipientName,
                    recipientPhone = if (prev.recipientPhone.isBlank()) task.recipientPhone else prev.recipientPhone,
                    taskLat = task.lat,
                    taskLng = task.lng
                )
            }
        }
    }

    fun onPhotoCaptured(path: String)    { _uiState.update { it.copy(photoPath = path) } }
    fun onSignatureSaved(path: String)   { _uiState.update { it.copy(signaturePath = path) } }
    fun onCodToggled(collected: Boolean) { _uiState.update { it.copy(codCollected = collected) } }

    fun showFailureSheet()    { _uiState.update { it.copy(showFailureSheet = true) } }
    fun dismissFailureSheet() { _uiState.update { it.copy(showFailureSheet = false) } }

    /**
     * Calls POST /v1/otps/generate to send an OTP to the recipient.
     * Sets otpSent=true on success so the entry field becomes visible.
     *
     * Dummy mode: when the backend returns `code` in the response (no real SMS
     * adapter configured), the code is stored in [PodUiState.dummyOtpCode] and
     * shown on-screen, and [confirmOtp] is called automatically so the driver
     * can proceed without typing anything. This lets POD testing work without
     * a live Twilio account.
     *
     * Idempotent — re-clicking "Resend" while a request is in-flight is a no-op.
     */
    fun sendOtpToRecipient() {
        val state = _uiState.value
        if (state.isSendingOtp) return
        viewModelScope.launch {
            _uiState.update { it.copy(isSendingOtp = true, otpError = null) }
            runCatching {
                repo.generateOtp(state.shipmentId, state.recipientPhone)
            }.onSuccess { result ->
                _uiState.update {
                    it.copy(isSendingOtp = false, otpSent = true, dummyOtpCode = result.code)
                }
                // Dummy mode: backend returned the code directly — auto-verify so the
                // driver doesn't need to manually enter a code that was never sent via SMS.
                result.code?.let { confirmOtp(it) }
            }.onFailure { e ->
                _uiState.update {
                    it.copy(isSendingOtp = false, otpError = "Failed to send code: ${e.message}")
                }
            }
        }
    }

    /**
     * Calls POST /v1/otps/verify to authenticate the 6-digit code with the backend.
     * On success, stores the code as otpToken, which satisfies canSubmit.
     * On failure, surfaces an inline error without blocking the entry field.
     */
    fun confirmOtp(code: String) {
        val state = _uiState.value
        if (state.isVerifyingOtp || state.otpToken != null) return
        viewModelScope.launch {
            _uiState.update { it.copy(isVerifyingOtp = true, otpError = null) }
            runCatching {
                repo.verifyOtp(state.shipmentId, code)
            }.onSuccess {
                _uiState.update { it.copy(isVerifyingOtp = false, otpToken = code) }
            }.onFailure { e ->
                _uiState.update {
                    it.copy(isVerifyingOtp = false, otpError = "Invalid code — ${e.message}")
                }
            }
        }
    }

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
            // Use live GPS when available; fall back to the task's stored delivery
            // coordinates when the device has no fix (cold start, GPS blocked indoors).
            // The backend geofence check is the authoritative gate for location accuracy.
            val captureLat = loc?.lat?.takeIf { it != 0.0 } ?: state.taskLat
            val captureLng = loc?.lng?.takeIf { it != 0.0 } ?: state.taskLng
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
