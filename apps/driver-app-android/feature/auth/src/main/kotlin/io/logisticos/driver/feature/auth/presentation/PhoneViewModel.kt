package io.logisticos.driver.feature.auth.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import io.logisticos.driver.core.network.auth.SessionManager
import io.logisticos.driver.feature.auth.data.AuthRepository
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import javax.inject.Inject

data class PhoneUiState(
    val isEmailMode: Boolean = false,
    val phone: String = "",
    val email: String = "",
    val isLoading: Boolean = false,
    val error: String? = null,
    val otpSent: Boolean = false,
    /** Runtime tenant slug — set by invite deep link or manual company code entry. */
    val tenantSlug: String = "",
    /** When true, the phone field is locked (pre-filled from invite link). */
    val phoneLocked: Boolean = false,
    /** Free-text company code entered manually when there is no invite deep link. */
    val companyCode: String = "",
    val companyCodeError: String? = null,
) {
    val identifier: String get() = if (isEmailMode) email else phone

    /** OTP can only be sent when we have a resolved tenant slug. */
    val canSend: Boolean get() = tenantSlug.isNotBlank()
}

@HiltViewModel
class PhoneViewModel @Inject constructor(
    private val repo: AuthRepository,
    private val sessionManager: SessionManager,
) : ViewModel() {

    private val _uiState = MutableStateFlow(PhoneUiState())
    val uiState = _uiState.asStateFlow()

    /**
     * Called once from PhoneScreen's LaunchedEffect(Unit). Reads any pending
     * invite stored by MainActivity when the deep link was tapped, pre-fills
     * phone + slug, and immediately clears the pending invite so a screen
     * rotation doesn't double-load.
     */
    fun loadPendingInvite() {
        val invite = sessionManager.getPendingInvite() ?: return
        val (slug, phone, _) = invite
        sessionManager.clearPendingInvite()
        preloadInvite(slug, phone)
    }

    fun onToggleMode(emailMode: Boolean) {
        _uiState.update { it.copy(isEmailMode = emailMode, error = null) }
    }

    fun onPhoneChanged(value: String) {
        _uiState.update { it.copy(phone = value, error = null) }
    }

    fun onEmailChanged(value: String) {
        _uiState.update { it.copy(email = value, error = null) }
    }

    fun onCompanyCodeChanged(value: String) {
        _uiState.update { it.copy(companyCode = value, companyCodeError = null) }
    }

    /**
     * Called by PhoneScreen when it reads a pending invite from SessionManager.
     * Pre-fills phone + slug and locks the phone field.
     */
    fun preloadInvite(slug: String, phone: String) {
        _uiState.update {
            it.copy(
                tenantSlug  = slug,
                phone       = phone,
                phoneLocked = true,
                companyCode = "",
                error       = null,
            )
        }
    }

    /**
     * Resolves the tenant slug from the manually entered company code.
     * v1: company code IS the tenant slug (no lookup round-trip needed).
     * v2: could do a GET /v1/tenants/by-code if slugs become opaque.
     */
    fun applyCompanyCode() {
        val code = _uiState.value.companyCode.trim().lowercase()
        if (code.isBlank()) {
            _uiState.update { it.copy(companyCodeError = "Enter your company code") }
            return
        }
        _uiState.update { it.copy(tenantSlug = code, companyCodeError = null, error = null) }
    }

    fun sendOtp() {
        val state = _uiState.value
        val slug = state.tenantSlug

        if (slug.isBlank()) {
            _uiState.update { it.copy(error = "Enter your company code to continue") }
            return
        }

        if (state.isEmailMode) {
            val email = state.email.trim()
            if (!email.contains("@") || !email.contains(".")) {
                _uiState.update { it.copy(error = "Enter a valid email address") }
                return
            }
            viewModelScope.launch {
                _uiState.update { it.copy(isLoading = true, error = null) }
                try {
                    repo.sendOtp(email = email, tenantSlug = slug)
                        .onSuccess { _uiState.update { it.copy(isLoading = false, otpSent = true) } }
                        .onFailure { e -> _uiState.update { it.copy(isLoading = false, error = e.message ?: "Something went wrong") } }
                } finally {
                    _uiState.update { if (it.isLoading) it.copy(isLoading = false) else it }
                }
            }
        } else {
            val phone = state.phone.trim()
            if (phone.length < 10) {
                _uiState.update { it.copy(error = "Enter a valid phone number") }
                return
            }
            viewModelScope.launch {
                _uiState.update { it.copy(isLoading = true, error = null) }
                try {
                    repo.sendOtp(phone = phone, tenantSlug = slug)
                        .onSuccess { _uiState.update { it.copy(isLoading = false, otpSent = true) } }
                        .onFailure { e -> _uiState.update { it.copy(isLoading = false, error = e.message ?: "Something went wrong") } }
                } finally {
                    _uiState.update { if (it.isLoading) it.copy(isLoading = false) else it }
                }
            }
        }
    }

    /** Call this immediately after consuming the otpSent event so that
     *  navigating back to PhoneScreen does not fire a stale re-navigation. */
    fun onOtpSentConsumed() {
        _uiState.update { it.copy(otpSent = false) }
    }
}
