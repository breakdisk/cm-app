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

data class PhoneUiState(
    val phone: String = "",
    val isLoading: Boolean = false,
    val error: String? = null,
    val otpSent: Boolean = false
)

@HiltViewModel
class PhoneViewModel @Inject constructor(
    private val repo: AuthRepository
) : ViewModel() {

    private val _uiState = MutableStateFlow(PhoneUiState())
    val uiState = _uiState.asStateFlow()

    fun onPhoneChanged(value: String) {
        _uiState.update { it.copy(phone = value, error = null) }
    }

    fun sendOtp() {
        val phone = _uiState.value.phone.trim()
        if (phone.length < 10) {
            _uiState.update { it.copy(error = "Enter a valid phone number") }
            return
        }
        viewModelScope.launch {
            _uiState.update { it.copy(isLoading = true, error = null) }
            repo.sendOtp(phone)
                .onSuccess { _uiState.update { it.copy(isLoading = false, otpSent = true) } }
                .onFailure { e -> _uiState.update { it.copy(isLoading = false, error = e.message ?: "Something went wrong") } }
        }
    }
}
