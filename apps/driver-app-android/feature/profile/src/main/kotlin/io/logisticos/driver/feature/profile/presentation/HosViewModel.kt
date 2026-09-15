package io.logisticos.driver.feature.profile.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import io.logisticos.driver.core.network.service.DriverOpsApiService
import io.logisticos.driver.core.network.service.HosData
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import javax.inject.Inject

data class HosUiState(
    val loading: Boolean = false,
    /**
     * Null until driver-ops answers — and left null when it cannot (an older
     * driver-ops without the endpoint, no signal), so no invented clock is shown.
     */
    val clock: HosData? = null,
    /** When [clock] was fetched; the screen counts on from here while on duty. */
    val fetchedAtMillis: Long = 0L,
)

/**
 * Hours-of-service clock (GET /v1/drivers/me/hos). Screens call [refresh] when
 * they appear and whenever duty changes; there is no poll — while the driver is
 * on duty the screen adds the minutes elapsed since [HosUiState.fetchedAtMillis].
 */
@HiltViewModel
class HosViewModel @Inject constructor(
    private val api: DriverOpsApiService,
) : ViewModel() {

    private val _uiState = MutableStateFlow(HosUiState())
    val uiState: StateFlow<HosUiState> = _uiState.asStateFlow()

    fun refresh() {
        viewModelScope.launch {
            _uiState.update { it.copy(loading = true) }
            val result = runCatching { api.getMyHos().data }
            _uiState.update { state ->
                result.fold(
                    onSuccess = { clock ->
                        HosUiState(loading = false, clock = clock, fetchedAtMillis = System.currentTimeMillis())
                    },
                    // Keep the last good clock; with none, show none.
                    onFailure = { state.copy(loading = false) },
                )
            }
        }
    }
}
