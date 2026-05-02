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
    val isSuccess: Boolean = false
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

    fun resendOtp(identifier: String) {
        viewModelScope.launch {
            _uiState.update { it.copy(error = null) }
            val result = if (identifier.contains("@"))
                repo.sendOtp(email = identifier)
            else
                repo.sendOtp(phone = identifier)
            result.onFailure { e -> _uiState.update { it.copy(error = e.message ?: "Failed to resend OTP") } }
        }
    }

    fun verifyOtp(identifier: String, otp: String) {
        viewModelScope.launch {
            _uiState.update { it.copy(isLoading = true, error = null) }
            val result = if (identifier.contains("@"))
                repo.verifyOtp(email = identifier, otp = otp)
            else
                repo.verifyOtp(phone = identifier, otp = otp)
            result
                .onSuccess { _uiState.update { it.copy(isLoading = false, isSuccess = true) } }
                .onFailure { e -> _uiState.update { it.copy(isLoading = false, error = e.message ?: "Invalid OTP") } }
        }
    }
}
