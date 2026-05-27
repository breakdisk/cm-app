package io.logisticos.driver.feature.profile.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import io.logisticos.driver.core.network.auth.SessionManager
import io.logisticos.driver.core.network.service.IdentityApiService
import okhttp3.OkHttpClient
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import javax.inject.Inject

data class ProfileUiState(
    val displayName: String = "",
    val email: String = "",
    val phone: String = "",
    /** Raw IDs from SessionManager — shown as fallback while profile loads. */
    val driverId: String = "",
    val tenantId: String = "",
    val isOfflineMode: Boolean = false,
    val isLoading: Boolean = false
)

@HiltViewModel
class ProfileViewModel @Inject constructor(
    val sessionManager: SessionManager,
    private val identityApi: IdentityApiService,
    private val okHttpClient: OkHttpClient,
) : ViewModel() {

    private val _uiState = MutableStateFlow(
        ProfileUiState(
            driverId = sessionManager.getDriverId() ?: "",
            tenantId = sessionManager.getTenantId() ?: "",
            isOfflineMode = sessionManager.isOfflineModeActive()
        )
    )
    val uiState: StateFlow<ProfileUiState> = _uiState.asStateFlow()

    init {
        // Don't block profile loading on offline mode — the request will simply
        // fail silently and the raw IDs from SessionManager remain visible.
        loadProfile()
    }

    private fun loadProfile() {
        viewModelScope.launch {
            _uiState.update { it.copy(isLoading = true) }
            runCatching { identityApi.getMe() }
                .onSuccess { resp ->
                    val user = resp.data
                    _uiState.update {
                        it.copy(
                            displayName = "${user.firstName} ${user.lastName}".trim(),
                            email       = user.email,
                            phone       = user.phoneNumber ?: "",
                            isLoading   = false
                        )
                    }
                }
                .onFailure {
                    // Profile fetch is best-effort — silently fall back to IDs.
                    _uiState.update { it.copy(isLoading = false) }
                }
        }
    }

    /**
     * Cancels all in-flight OkHttp calls THEN clears the session.
     * Order matters: cancelAll() unblocks any TokenAuthenticator.runBlocking
     * threads that occupy dispatcher slots waiting for a token refresh — without
     * this, sendOtp on the login screen queues forever behind them.
     */
    fun logout() {
        okHttpClient.dispatcher.cancelAll()
        sessionManager.clearSession()
    }
}
