package io.logisticos.driver.feature.auth.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import io.logisticos.driver.feature.auth.data.AuthRepository
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import javax.inject.Inject

data class OtpUiState(
    val otp: String = "",
    val isLoading: Boolean = false,
    val error: String? = null,
    val isSuccess: Boolean = false,
    /** Runtime tenant slug — forwarded from PhoneScreen via nav arg. */
    val tenantSlug: String = "",
)

@HiltViewModel
class OtpViewModel @Inject constructor(
    private val repo: AuthRepository
) : ViewModel() {

    private val _uiState = MutableStateFlow(OtpUiState())
    val uiState = _uiState.asStateFlow()

    fun onOtpChanged(value: String) {
        if (value.length <= 6) _uiState.update { it.copy(otp = value, error = null) }
    }

    /** Must be called once from OtpScreen's LaunchedEffect before any OTP action. */
    fun setTenantSlug(slug: String) {
        _uiState.update { it.copy(tenantSlug = slug) }
    }

    fun resendOtp(identifier: String) {
        val slug = _uiState.value.tenantSlug
        viewModelScope.launch {
            _uiState.update { it.copy(error = null) }
            val result = if (identifier.contains("@"))
                repo.sendOtp(email = identifier, tenantSlug = slug)
            else
                repo.sendOtp(phone = identifier, tenantSlug = slug)
            result.onFailure { e -> _uiState.update { it.copy(error = e.message ?: "Failed to resend OTP") } }
        }
    }

    fun verifyOtp(identifier: String, otp: String) {
        val slug = _uiState.value.tenantSlug
        viewModelScope.launch {
            _uiState.update { it.copy(isLoading = true, error = null) }
            try {
                val result = if (identifier.contains("@"))
                    repo.verifyOtp(email = identifier, otp = otp, tenantSlug = slug)
                else
                    repo.verifyOtp(phone = identifier, otp = otp, tenantSlug = slug)
                result
                    .onSuccess { _uiState.update { it.copy(isLoading = false, isSuccess = true) } }
                    .onFailure { e -> _uiState.update { it.copy(isLoading = false, error = e.message ?: "Invalid OTP") } }
            } finally {
                // Guarantee spinner is cleared on cancellation (back-press mid-request).
                _uiState.update { if (it.isLoading) it.copy(isLoading = false) else it }
            }
        }
    }

    /** Call after consuming isSuccess so back-navigation can't re-trigger it. */
    fun onSuccessConsumed() {
        _uiState.update { it.copy(isSuccess = false) }
    }
}
