package io.logisticos.driver.feature.profile.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import io.logisticos.driver.core.network.auth.SessionManager
import io.logisticos.driver.core.network.service.DriverOpsApiService
import io.logisticos.driver.core.network.service.IdentityApiService
import io.logisticos.driver.core.network.service.PaymentsApiService
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
    val isLoading: Boolean = false,
    // ── Financial section ────────────────────────────────────────────────────
    /** True for part-time (gig) drivers — gates the Earnings card. Full-time
     *  drivers never see payout figures (only the COD cash position). */
    val isGigWorker: Boolean = false,
    val todayCents: Long = 0L,
    val weekCents: Long = 0L,
    /** Cents the driver currently owes the hub (open COD ledger balance). */
    val openBalanceCents: Long = 0L,
    /** Number of COD/pickup debits behind that balance — "from N deliveries". */
    val openDebitCount: Int = 0,
)

@HiltViewModel
class ProfileViewModel @Inject constructor(
    val sessionManager: SessionManager,
    private val identityApi: IdentityApiService,
    private val driverOpsApi: DriverOpsApiService,
    private val paymentsApi: PaymentsApiService,
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
        loadFinancials()
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
     * Financial summary — three independent best-effort fetches. Each failure
     * leaves its section at the zero default (cards hide on zero), so a
     * payments outage never breaks the profile screen.
     */
    private fun loadFinancials() {
        viewModelScope.launch {
            runCatching { driverOpsApi.getMyProfile() }
                .onSuccess { r ->
                    _uiState.update { it.copy(isGigWorker = r.data.driverType == "part_time") }
                }
        }
        viewModelScope.launch {
            runCatching { driverOpsApi.getMyEarnings(limit = 1) }
                .onSuccess { r ->
                    _uiState.update { it.copy(
                        todayCents = r.data.todayCents,
                        weekCents  = r.data.weekCents,
                    ) }
                }
        }
        viewModelScope.launch {
            runCatching { paymentsApi.getMyLedger() }
                .onSuccess { r ->
                    _uiState.update { it.copy(
                        openBalanceCents = r.data.openBalanceCents,
                        openDebitCount   = r.data.openEntries.count { e -> e.amountCents > 0 },
                    ) }
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
