package io.logisticos.driver.feature.profile.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import io.logisticos.driver.core.network.service.DriverOpsApiService
import io.logisticos.driver.core.network.service.EarningsData
import io.logisticos.driver.core.network.service.DriverLedgerData
import io.logisticos.driver.core.network.service.PaymentsApiService
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import javax.inject.Inject

data class EarningsUiState(
    val isLoading: Boolean = true,
    val error: String? = null,
    /** Gates the Earnings tab — full-time drivers see only the Cash tab. */
    val isGigWorker: Boolean = false,
    val earnings: EarningsData? = null,
    val ledger: DriverLedgerData? = null,
)

/**
 * Full Earnings screen state: per-task payout history (driver-ops, reads the
 * contractual payout_cents snapshot) + COD cash position (payments ledger).
 * Two systems of record, deliberately not merged.
 */
@HiltViewModel
class EarningsViewModel @Inject constructor(
    private val driverOpsApi: DriverOpsApiService,
    private val paymentsApi: PaymentsApiService,
) : ViewModel() {

    private val _uiState = MutableStateFlow(EarningsUiState())
    val uiState: StateFlow<EarningsUiState> = _uiState.asStateFlow()

    init {
        refresh()
    }

    fun refresh() {
        viewModelScope.launch {
            _uiState.update { it.copy(isLoading = true, error = null) }

            runCatching { driverOpsApi.getMyProfile() }
                .onSuccess { r ->
                    _uiState.update { it.copy(isGigWorker = r.data.driverType == "part_time") }
                }

            val earningsResult = runCatching { driverOpsApi.getMyEarnings(limit = 100) }
            val ledgerResult   = runCatching { paymentsApi.getMyLedger() }

            _uiState.update {
                it.copy(
                    isLoading = false,
                    earnings  = earningsResult.getOrNull()?.data ?: it.earnings,
                    ledger    = ledgerResult.getOrNull()?.data ?: it.ledger,
                    // Only surface an error when BOTH sources failed — a
                    // partial view beats an error wall.
                    error = if (earningsResult.isFailure && ledgerResult.isFailure)
                        (earningsResult.exceptionOrNull()?.message ?: "Failed to load")
                    else null,
                )
            }
        }
    }
}
