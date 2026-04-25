package io.logisticos.driver.feature.home.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import io.logisticos.driver.core.database.dao.SyncQueueDao
import io.logisticos.driver.core.database.entity.ShiftEntity
import io.logisticos.driver.core.location.LocationRepository
import io.logisticos.driver.core.network.service.DriverOpsApiService
import io.logisticos.driver.core.network.service.UpdateLocationRequest
import io.logisticos.driver.feature.home.data.ShiftRepository
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
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
    val isOfflineMode: Boolean = false,
    /** Number of items waiting in the local outbound sync queue. Surfaces
     *  silent retry state to the driver — without this the screen lies
     *  ("submitted ✓") while the actual server hand-off is still pending. */
    val pendingSyncCount: Int = 0,
    /** True after the user has explicitly denied location permission at
     *  runtime (Android 11+ "Don't ask again" path). Drives the rationale
     *  card on the home screen. */
    val locationDenied: Boolean = false,
    /** Set true when the driver has just transitioned online → offline
     *  (manual toggle), and `shift` has data worth showing. The screen
     *  renders an end-of-shift summary dialog and calls dismissShiftSummary()
     *  on close. NOT set on initial offline state — only on the toggle. */
    val showShiftSummary: Boolean = false,
)

@HiltViewModel
class HomeViewModel @Inject constructor(
    private val repo: ShiftRepository,
    private val api: DriverOpsApiService,
    private val locationRepo: LocationRepository,
    private val syncQueueDao: SyncQueueDao,
) : ViewModel() {

    private val _uiState = MutableStateFlow(HomeUiState())
    val uiState: StateFlow<HomeUiState> = _uiState.asStateFlow()

    private var heartbeatJob: Job? = null

    init {
        viewModelScope.launch {
            repo.observeActiveShift().collect { shift ->
                _uiState.update { it.copy(shift = shift) }
            }
        }
        // Reactive — Room emits a new value any time enqueue/remove run.
        viewModelScope.launch {
            syncQueueDao.getPendingCount().collect { n ->
                _uiState.update { it.copy(pendingSyncCount = n) }
            }
        }
        syncShift()
        startPolling()
        autoGoOnline()
    }

    /** Called from HomeScreen when the OS reports a location-permission denial.
     *  Surfaces a rationale card; the user can retry from there. */
    fun onLocationPermissionDenied() {
        _uiState.update { it.copy(locationDenied = true) }
    }

    private fun startPolling() {
        viewModelScope.launch {
            while (true) {
                delay(30_000L)
                if (_uiState.value.isOnline) {
                    runCatching { repo.syncShift() }
                }
            }
        }
    }

    fun syncShift() {
        viewModelScope.launch {
            _uiState.update { it.copy(isLoading = true, error = null) }
            runCatching { repo.syncShift() }
                .onFailure { e -> _uiState.update { it.copy(error = e.message, isOfflineMode = true) } }
                .onSuccess { _uiState.update { it.copy(isOfflineMode = false) } }
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
                    pushFreshLocation()
                    syncShift()
                } else {
                    api.goOffline()
                }
            }.onSuccess {
                _uiState.update {
                    it.copy(
                        isOnline = goingOnline,
                        // Trigger end-of-shift summary on the online → offline edge.
                        // Skip the dialog if there's no shift loaded (cold app
                        // start, going offline before any work) — nothing to show.
                        showShiftSummary = !goingOnline && it.shift != null,
                    )
                }
                if (goingOnline) startLocationHeartbeat() else stopLocationHeartbeat()
            }.onFailure { e ->
                _uiState.update { it.copy(error = e.message) }
            }
            _uiState.update { it.copy(isTogglingStatus = false) }
        }
    }

    fun dismissShiftSummary() {
        _uiState.update { it.copy(showShiftSummary = false) }
    }

    // ── Availability ─────────────────────────────────────────────────────────
    //
    // Two invariants dispatch relies on when picking a driver for a new
    // shipment (services/dispatch/src/infrastructure/db/driver_avail_repo.rs):
    //   1. drivers.status = 'available'   — flipped by POST /v1/drivers/go-online
    //   2. last location ping within 10 minutes — else distance = ∞ and the
    //      driver ranks last / gets filtered out of the pool.
    //
    // Without these running automatically the driver app would appear to
    // "work" (login succeeds, task list renders) while dispatch silently
    // skips the driver and shipments queue forever. Auto-go-online on app
    // entry + a 60 s GPS heartbeat while online closes that gap. go-online
    // is idempotent server-side, so re-calling it on each launch is safe.

    /** Push a fresh GPS fix (up to 5 s) to driver-ops; falls back to last known. */
    private suspend fun pushFreshLocation() {
        locationRepo.getCurrentOrLastKnownLocation()?.let { loc ->
            api.updateLocation(
                UpdateLocationRequest(
                    lat = loc.lat,
                    lng = loc.lng,
                    recordedAt = DateTimeFormatter.ISO_INSTANT.format(Instant.now())
                )
            )
        }
    }

    private fun autoGoOnline() {
        viewModelScope.launch {
            runCatching {
                api.goOnline()
                pushFreshLocation()
            }.onSuccess {
                _uiState.update { it.copy(isOnline = true) }
                startLocationHeartbeat()
            }
            // On failure we stay offline-in-UI; the manual toggle is still there.
        }
    }

    /**
     * Called from HomeScreen once ACCESS_FINE/COARSE_LOCATION is granted at runtime.
     * Pushes a fresh fix immediately so the driver is discoverable by dispatch
     * without waiting up to 60 s for the next heartbeat tick.
     */
    fun onLocationPermissionGranted() {
        // Granting clears any prior denial banner.
        _uiState.update { it.copy(locationDenied = false) }
        viewModelScope.launch {
            runCatching { pushFreshLocation() }
        }
    }

    private fun startLocationHeartbeat() {
        heartbeatJob?.cancel()
        heartbeatJob = viewModelScope.launch {
            while (true) {
                delay(60_000L)
                runCatching { pushFreshLocation() }
            }
        }
    }

    private fun stopLocationHeartbeat() {
        heartbeatJob?.cancel()
        heartbeatJob = null
    }

    override fun onCleared() {
        stopLocationHeartbeat()
        super.onCleared()
    }
}
