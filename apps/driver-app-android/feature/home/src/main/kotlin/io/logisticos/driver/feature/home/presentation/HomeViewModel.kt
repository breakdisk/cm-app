package io.logisticos.driver.feature.home.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import io.logisticos.driver.core.database.entity.ShiftEntity
import io.logisticos.driver.core.location.LocationRepository
import io.logisticos.driver.core.network.service.DriverOpsApiService
import io.logisticos.driver.core.network.service.UpdateLocationRequest
import io.logisticos.driver.feature.home.data.ShiftRepository
import kotlinx.coroutines.flow.*
import kotlinx.coroutines.launch
import java.time.Instant
import java.time.format.DateTimeFormatter
import javax.inject.Inject

data class HomeUiState(
    val shift: ShiftEntity? = null,
    val isLoading: Boolean = false,
    val isOnline: Boolean = false,
    val isTogglingStatus: Boolean = false,
    val error: String? = null,
    val isOfflineMode: Boolean = false
)

@HiltViewModel
class HomeViewModel @Inject constructor(
    private val repo: ShiftRepository,
    private val api: DriverOpsApiService,
    private val locationRepo: LocationRepository
) : ViewModel() {

    private val _uiState = MutableStateFlow(HomeUiState())
    val uiState: StateFlow<HomeUiState> = _uiState.asStateFlow()

    init {
        viewModelScope.launch {
            repo.observeActiveShift().collect { shift ->
                _uiState.update { it.copy(shift = shift) }
            }
        }
        syncShift()
    }

    fun syncShift() {
        viewModelScope.launch {
            _uiState.update { it.copy(isLoading = true) }
            runCatching { repo.syncShift() }
                .onFailure { e -> _uiState.update { it.copy(error = e.message, isOfflineMode = true) } }
            _uiState.update { it.copy(isLoading = false) }
        }
    }

    fun toggleOnlineStatus() {
        val goingOnline = !_uiState.value.isOnline
        viewModelScope.launch {
            _uiState.update { it.copy(isTogglingStatus = true, error = null) }
            runCatching {
                if (goingOnline) {
                    api.goOnline()
                    // Push current location immediately so dispatch can find this driver
                    locationRepo.getLastKnownLocation()?.let { loc ->
                        api.updateLocation(
                            UpdateLocationRequest(
                                lat = loc.lat,
                                lng = loc.lng,
                                recordedAt = DateTimeFormatter.ISO_INSTANT.format(Instant.now())
                            )
                        )
                    }
                    syncShift()
                } else {
                    api.goOffline()
                }
            }.onSuccess {
                _uiState.update { it.copy(isOnline = goingOnline) }
            }.onFailure { e ->
                _uiState.update { it.copy(error = e.message) }
            }
            _uiState.update { it.copy(isTogglingStatus = false) }
        }
    }
}
