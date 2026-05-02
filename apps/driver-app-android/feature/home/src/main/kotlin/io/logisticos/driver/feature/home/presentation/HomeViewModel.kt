package io.logisticos.driver.feature.home.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import io.logisticos.driver.core.common.TaskSyncBus
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
import kotlinx.coroutines.withTimeoutOrNull
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
    /** True when the driver is online but all GPS fix attempts returned null
     *  (GPS cold-start, no cached fix, phone indoors). Clears automatically
     *  as soon as any successful location push completes — either via the
     *  retry loop or the next foreground service heartbeat. */
    val gpsUnavailable: Boolean = false,
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
        collectSyncBus()
        collectLocationUpdates()
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
                runCatching { repo.syncShift() }
            }
        }
    }

    private fun collectSyncBus() {
        viewModelScope.launch {
            TaskSyncBus.events.collect {
                runCatching { repo.syncShift() }
            }
        }
    }

    /**
     * Collect GPS fixes published by [LocationForegroundService] and forward
     * each one to the backend.  This is the primary path keeping
     * driver_ops.driver_locations up-to-date while a shift is running.
     *
     * The foreground service emits on every location callback (adaptive
     * interval: ~5 s moving, ~60 s stationary).  The ViewModel's own 60 s
     * heartbeat remains as a fallback for when the service isn't running.
     */
    private fun collectLocationUpdates() {
        viewModelScope.launch {
            locationRepo.locationUpdates.collect { loc ->
                runCatching {
                    api.updateLocation(
                        UpdateLocationRequest(
                            lat = loc.lat,
                            lng = loc.lng,
                            recordedAt = DateTimeFormatter.ISO_INSTANT.format(Instant.now())
                        )
                    )
                    // GPS is clearly working — clear any stale warning.
                    _uiState.update { it.copy(gpsUnavailable = false) }
                }
                // Failures are silent here; the foreground service will retry
                // on the next location update.  Offline-mode banner covers
                // persistent connectivity loss.
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
                    // Start the foreground service immediately so FusedLocation-
                    // Provider begins requesting continuous GPS/network fixes.
                    // Empty shiftId = availability mode: publishes to the
                    // locationUpdates SharedFlow without recording breadcrumbs.
                    // This is what populates driver_ops.driver_locations so
                    // dispatch's proximity query can find the driver.
                    locationRepo.startShiftTracking("")
                    pushFreshLocation()
                    syncShift()
                } else {
                    api.goOffline()
                    locationRepo.stopShiftTracking()
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

    /**
     * Push a fresh GPS fix to driver-ops.
     *
     * On a cold-start device [LocationRepository.getCurrentOrLastKnownLocation]
     * can return null because the FusedLocationProvider hasn't warmed up yet.
     * Retrying with a short delay lets the OS acquire a first fix from network-
     * assisted positioning (~2-4 s typical) before we give up.
     *
     * Retry policy:  3 attempts × 4 s apart → up to 8 s extra wait.
     * If all attempts fail we set [HomeUiState.gpsUnavailable] so the UI can
     * show a warning banner. The state clears automatically the moment any
     * successful push happens (retry, heartbeat, or foreground service fix).
     */
    private suspend fun pushFreshLocation() {
        val maxAttempts = 3
        val retryDelayMs = 4_000L
        repeat(maxAttempts) { attempt ->
            val loc = locationRepo.getCurrentOrLastKnownLocation()
            if (loc != null) {
                runCatching {
                    api.updateLocation(
                        UpdateLocationRequest(
                            lat = loc.lat,
                            lng = loc.lng,
                            recordedAt = DateTimeFormatter.ISO_INSTANT.format(Instant.now())
                        )
                    )
                    _uiState.update { it.copy(gpsUnavailable = false) }
                }
                return   // success — even if the API call failed, GPS is working
            }
            if (attempt < maxAttempts - 1) delay(retryDelayMs)
        }
        // All attempts returned null — GPS unavailable (indoors, cold start, etc.)
        _uiState.update { it.copy(gpsUnavailable = true) }
    }

    private fun autoGoOnline() {
        viewModelScope.launch {
            runCatching {
                api.goOnline()
                // Do NOT start LocationForegroundService here. On Android 14+,
                // startForeground(FOREGROUND_SERVICE_TYPE_LOCATION) throws a
                // RemoteException if ACCESS_FINE_LOCATION has not been granted
                // at runtime yet. HomeScreen requests the permission on entry and
                // calls onLocationPermissionGranted() once the OS confirms the
                // grant — that is the safe place to start tracking.
            }.onSuccess {
                _uiState.update { it.copy(isOnline = true) }
                startLocationHeartbeat()
            }
            // On failure we stay offline-in-UI; the manual toggle is still there.
        }
    }

    /**
     * Called from HomeScreen once ACCESS_FINE/COARSE_LOCATION is granted at runtime.
     * Starts the location foreground service (safe now that permission is held)
     * and pushes a fresh fix immediately so the driver is discoverable by dispatch
     * without waiting up to 60 s for the next heartbeat tick.
     */
    fun onLocationPermissionGranted() {
        // Granting clears any prior denial banner.
        _uiState.update { it.copy(locationDenied = false) }
        viewModelScope.launch {
            runCatching {
                // Start the foreground service now that location permission is confirmed.
                locationRepo.startShiftTracking("")   // availability mode — no breadcrumbs
                pushFreshLocation()
            }
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
