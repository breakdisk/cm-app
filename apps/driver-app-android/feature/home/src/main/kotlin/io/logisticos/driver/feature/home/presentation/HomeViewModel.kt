package io.logisticos.driver.feature.home.presentation

import android.content.Context
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import dagger.hilt.android.qualifiers.ApplicationContext
import io.logisticos.driver.core.common.AssignmentPayload
import io.logisticos.driver.core.common.PendingAssignmentBus
import io.logisticos.driver.core.common.TaskSyncBus
import io.logisticos.driver.core.database.dao.SyncQueueDao
import io.logisticos.driver.core.database.entity.ShiftEntity
import io.logisticos.driver.core.database.entity.SyncAction
import io.logisticos.driver.core.database.entity.SyncQueueEntity
import io.logisticos.driver.core.database.worker.OutboundSyncWorker
import io.logisticos.driver.core.location.LocationRepository
import io.logisticos.driver.core.network.service.ComplianceApiService
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
    /** Overall compliance status from the compliance service. Non-null once
     *  the first successful fetch completes. Null while loading or if the
     *  service is unreachable (banner stays hidden on error — non-blocking). */
    val complianceStatus: String? = null,
    /** True when the driver has the hub_scanner role. Gates Hub Mode button visibility. */
    val isHubScanner: Boolean = false,
    /** Hub UUID pre-filled into HubScanScreen. Empty string when no hub is assigned. */
    val hubId: String = "",
    /** Pending and in-progress tasks rendered as rich cards on the home screen. */
    val tasks: List<io.logisticos.driver.core.network.service.TaskItem> = emptyList(),
    /** Incoming assignment offer awaiting Accept/Decline. Mirrors
     *  PendingAssignmentBus so the offer card renders on Home even if the
     *  driver dismissed the full-screen AssignmentScreen. */
    val pendingOffer: AssignmentPayload? = null,
    /** True while the accept/decline network call is in flight. */
    val isActingOnOffer: Boolean = false,
    /** True while the on-screen grab offer was just lost to another driver —
     *  the card renders a brief "Taken" state, then auto-dismisses. */
    val offerTaken: Boolean = false,
    /** Seconds remaining on the grab offer's TTL (countdown ring). Null when
     *  the pending offer is a 1:1 assignment (no deadline). */
    val offerSecondsLeft: Int? = null,
    /** Gig acceptance-rate inputs from the profile (impression-verified). */
    val offersSeen: Long = 0L,
    val offersClaimed: Long = 0L,
    /** True when the driver is part-time (gig). Gates payout display and the
     *  decline counter — full-time drivers never see a price. */
    val isGigWorker: Boolean = false,
    /** Gig per-delivery rate in cents; null for full-time drivers so the
     *  offer card's payout chip stays hidden for them. */
    val gigRateCents: Long? = null,
    /** Decline counter from the backend (ban at [DECLINE_BAN_LIMIT]). */
    val declineCount: Int = 0,
    /** Customer rating aggregate; null until the first rating lands. */
    val ratingAvg: Float? = null,
    val ratingCount: Int = 0,
    /** Last known device position — used for distance/ETA on task cards. */
    val lastLat: Double? = null,
    val lastLng: Double? = null,
)

/** Declines at which a gig driver is automatically deactivated (backend rule). */
const val DECLINE_BAN_LIMIT = 20

/** Open-offer poll cadence. Must stay well under the backend 30 s offer TTL so
 *  a broadcast grab card appears even when its FCM push is missed. */
private const val OFFER_POLL_INTERVAL_MS = 6_000L

@HiltViewModel
class HomeViewModel @Inject constructor(
    @ApplicationContext private val context: Context,
    private val repo:          ShiftRepository,
    private val api:           DriverOpsApiService,
    private val complianceApi: ComplianceApiService,
    private val identityApi:   io.logisticos.driver.core.network.service.IdentityApiService,
    private val locationRepo:  LocationRepository,
    private val syncQueueDao:  SyncQueueDao,
    private val sessionManager: io.logisticos.driver.core.network.auth.SessionManager,
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
        // Mirror the FCM-driven offer bus into UI state so the home screen can
        // render an Accept/Decline card (in addition to the full-screen
        // AssignmentScreen flow, which still works).
        viewModelScope.launch {
            PendingAssignmentBus.pending.collect { offer ->
                _uiState.update { it.copy(pendingOffer = offer, offerTaken = false) }
                reportOfferSeen(offer)
            }
        }
        // Lost-race signal (FCM offer_closed or claim 409): flash "Taken" for
        // 1.5 s, then dismiss the card.
        viewModelScope.launch {
            PendingAssignmentBus.takenOfferId.collect { takenId ->
                if (takenId != null && _uiState.value.pendingOffer?.offerId == takenId) {
                    _uiState.update { it.copy(offerTaken = true) }
                    delay(1_500L)
                    PendingAssignmentBus.clearTaken()
                    PendingAssignmentBus.clear()
                }
            }
        }
        // Countdown ticker for the grab offer's TTL. Expiry dismisses the
        // card locally — the backend sweeper escalates/expires server-side.
        viewModelScope.launch {
            while (true) {
                delay(1_000L)
                val offer = _uiState.value.pendingOffer ?: continue
                val deadline = offer.expiresAtMillis ?: continue
                val left = ((deadline - System.currentTimeMillis()) / 1000L).toInt()
                if (left <= 0 && !_uiState.value.offerTaken) {
                    PendingAssignmentBus.clear()
                    _uiState.update { it.copy(offerSecondsLeft = null) }
                } else {
                    _uiState.update { it.copy(offerSecondsLeft = left.coerceAtLeast(0)) }
                }
            }
        }
        startOfferPolling()
        syncShift()
        loadTasks()
        startPolling()
        autoGoOnline()
        collectSyncBus()
        collectLocationUpdates()
        loadComplianceStatus()
        // Phase 1 — instant render from cache
        _uiState.update { it.copy(
            isHubScanner = sessionManager.isHubScanner(),
            hubId        = sessionManager.getHubId() ?: "",
        ) }
        // Phase 2 — silent background re-sync
        refreshHubProfile()
    }

    private fun loadComplianceStatus() {
        viewModelScope.launch {
            runCatching { complianceApi.getMyProfile() }
                .onSuccess { envelope ->
                    _uiState.update { it.copy(complianceStatus = envelope.data.profile.overallStatus) }
                }
            // On failure, complianceStatus stays null — banner stays hidden.
        }
    }

    /**
     * Silently re-fetches role and hub_id from the backend.
     * Called in init{} and from syncShift() so it fires on every foreground —
     * mid-shift role changes take effect without re-login.
     */
    private fun refreshHubProfile() {
        viewModelScope.launch {
            runCatching { identityApi.getMe() }
                .onSuccess { me ->
                    val isHub = me.data.roles.contains("hub_scanner")
                    sessionManager.saveIsHubScanner(isHub)
                    _uiState.update { it.copy(isHubScanner = isHub) }
                }
            runCatching { api.getMyProfile() }
                .onSuccess { r ->
                    sessionManager.saveHubId(r.data.hubId)
                    val isGig = r.data.driverType == "part_time"
                    _uiState.update { it.copy(
                        hubId         = r.data.hubId ?: "",
                        isGigWorker   = isGig,
                        gigRateCents  = if (isGig) r.data.perDeliveryRateCents.toLong() else null,
                        declineCount  = r.data.declineCount,
                        ratingAvg     = r.data.ratingAvg,
                        ratingCount   = r.data.ratingCount,
                        offersSeen    = r.data.offersSeen,
                        offersClaimed = r.data.offersClaimed,
                    ) }
                }
        }
    }

    /** Pulls the driver's pending/in-progress tasks for the home-screen cards. */
    private fun loadTasks() {
        viewModelScope.launch {
            runCatching { api.listMyTasks() }
                .onSuccess { r -> _uiState.update { it.copy(tasks = r.data) } }
            // On failure the previous list stays — offline banner covers connectivity.
        }
    }

    /** Accept the pending offer from the home card. Clears the bus on success
     *  so neither the card nor AssignmentScreen re-renders it. */
    fun acceptOffer() {
        val offer = _uiState.value.pendingOffer ?: return
        viewModelScope.launch {
            _uiState.update { it.copy(isActingOnOffer = true, error = null) }
            runCatching { api.acceptAssignment(offer.assignmentId) }
                .onSuccess {
                    PendingAssignmentBus.clear()
                    syncShift()
                    loadTasks()
                }
                .onFailure { e -> _uiState.update { it.copy(error = e.message) } }
            _uiState.update { it.copy(isActingOnOffer = false) }
        }
    }

    /** Decline the pending offer with a reason. The backend re-queues the
     *  shipment and counts the decline toward the ban threshold. */
    fun declineOffer(reason: String) {
        val offer = _uiState.value.pendingOffer ?: return
        viewModelScope.launch {
            _uiState.update { it.copy(isActingOnOffer = true, error = null) }
            runCatching {
                api.rejectAssignment(
                    offer.assignmentId,
                    io.logisticos.driver.core.network.service.RejectAssignmentRequest(reason),
                )
            }
                .onSuccess {
                    PendingAssignmentBus.clear()
                    // Optimistic counter bump; the next profile sync corrects it.
                    _uiState.update { it.copy(declineCount = it.declineCount + 1) }
                    refreshHubProfile()
                }
                .onFailure { e -> _uiState.update { it.copy(error = e.message) } }
            _uiState.update { it.copy(isActingOnOffer = false) }
        }
    }

    // ── Gig offer ("grab") flow ──────────────────────────────────────────────

    /** Offer ids whose impression was already reported — fire once per offer. */
    private val seenOfferIds = mutableSetOf<String>()

    /**
     * Client-verified impression: the card actually rendered on THIS phone.
     * The backend counts only these in the acceptance-rate denominator, so a
     * push that died in a dead zone never penalizes the driver. Fire-and-
     * forget — no offline queue: a 30 s offer that can't report an impression
     * couldn't be grabbed either.
     */
    private fun reportOfferSeen(offer: AssignmentPayload?) {
        val offerId = offer?.offerId ?: return
        if (offerId.isBlank() || !seenOfferIds.add(offerId)) return
        viewModelScope.launch {
            runCatching { api.markOfferSeen(offerId) }
        }
    }

    /**
     * The grab. Optimistic UI: fires immediately on tap; a 409 means another
     * driver won (or we already hold an assignment) — flip to "Taken".
     */
    fun claimOffer() {
        val offer = _uiState.value.pendingOffer ?: return
        if (!offer.isGrabOffer || _uiState.value.isActingOnOffer) return
        viewModelScope.launch {
            _uiState.update { it.copy(isActingOnOffer = true, error = null) }
            runCatching { api.claimOffer(offer.offerId) }
                .onSuccess {
                    PendingAssignmentBus.clear()
                    syncShift()
                    loadTasks()
                }
                .onFailure { e ->
                    if ((e as? retrofit2.HttpException)?.code() == 409) {
                        // Lost the race — honest, brief "Taken" feedback.
                        PendingAssignmentBus.markTaken(offer.offerId)
                    } else {
                        _uiState.update { it.copy(error = e.message) }
                    }
                }
            _uiState.update { it.copy(isActingOnOffer = false) }
        }
    }

    /** Penalty-free pass: never re-offered this task; decline_count untouched. */
    fun passOffer() {
        val offer = _uiState.value.pendingOffer ?: return
        if (!offer.isGrabOffer) return
        viewModelScope.launch {
            runCatching { api.passOffer(offer.offerId) }
            PendingAssignmentBus.clear()
        }
    }

    /**
     * Pull the driver's live broadcast offers from the backend and surface the
     * freshest as a grab card. This is the resilience path: FCM `task_offer`
     * pushes are primary, but they're best-effort (and unconfigured on some
     * deployments), so without an active poll a broadcast offer would only ever
     * appear if the app happened to cold-start during its 30 s life. Polled on
     * a tight cadence (shorter than the offer TTL) so a grab card actually
     * shows during normal testing even when no push arrives.
     *
     * Only posts when nothing is already on screen — a live FCM offer, an
     * in-flight claim, or the brief "Taken" flash all take precedence. Posting
     * the same offer id again is idempotent (StateFlow dedupes equal values),
     * so the countdown never resets mid-tick.
     */
    private fun fetchAndPostOpenOffer() {
        viewModelScope.launch {
            if (PendingAssignmentBus.pending.value != null) return@launch
            runCatching { api.listOpenOffers() }
                .onSuccess { r ->
                    val offer = r.data.firstOrNull() ?: return@onSuccess
                    if (PendingAssignmentBus.pending.value != null) return@onSuccess
                    val expiresMs = runCatching {
                        java.time.OffsetDateTime.parse(offer.expiresAt).toInstant().toEpochMilli()
                    }.getOrNull()
                    PendingAssignmentBus.post(
                        AssignmentPayload(
                            assignmentId     = "",
                            shipmentId       = offer.shipmentId,
                            customerName     = offer.customerName,
                            address          = offer.deliveryAddress,
                            taskType         = "delivery",
                            trackingNumber   = offer.trackingNumber,
                            codAmountCents   = offer.codAmountCents ?: 0L,
                            merchantName     = offer.merchantName,
                            deliveryCategory = offer.deliveryCategory,
                            weightGrams      = offer.weightGrams,
                            pickupLat        = offer.pickupLat,
                            pickupLng        = offer.pickupLng,
                            deliveryLat      = offer.deliveryLat,
                            deliveryLng      = offer.deliveryLng,
                            offerId          = offer.id,
                            payoutCents      = offer.payoutCents,
                            expiresAtMillis  = expiresMs,
                        )
                    )
                }
            // Failure is silent — FCM remains the primary delivery path.
        }
    }

    /**
     * Poll open offers every [OFFER_POLL_INTERVAL_MS]. Granularity is well
     * under the 30 s offer TTL so a broadcast surfaces as a grab card within a
     * few seconds even if its FCM push never lands. Skips work while an offer
     * is already on screen (the poll guard inside fetchAndPostOpenOffer).
     */
    private fun startOfferPolling() {
        viewModelScope.launch {
            while (true) {
                // Only gig (part-time) drivers ever receive broadcast offers —
                // skip the poll for full-timers so we don't add needless load.
                if (_uiState.value.isGigWorker) fetchAndPostOpenOffer()
                delay(OFFER_POLL_INTERVAL_MS)
            }
        }
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
                loadTasks()
            }
        }
    }

    private fun collectSyncBus() {
        viewModelScope.launch {
            TaskSyncBus.events.collect {
                runCatching { repo.syncShift() }
                loadTasks()
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
                // Keep the freshest fix in state — task cards compute
                // distance-to-pickup and ETA from it.
                _uiState.update { it.copy(lastLat = loc.lat, lastLng = loc.lng) }
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
            loadComplianceStatus()
            refreshHubProfile()
            loadTasks()
        }
    }

    fun toggleOnlineStatus() {
        val goingOnline = !_uiState.value.isOnline
        viewModelScope.launch {
            _uiState.update { it.copy(isTogglingStatus = true, error = null) }
            val result = runCatching {
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
            }
            result.onSuccess {
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
            }
            if (result.isFailure) {
                _uiState.update { it.copy(error = result.exceptionOrNull()?.message) }
                // Network error — enqueue SHIFT_START / SHIFT_END so the status
                // flips server-side once connectivity returns. OutboundSyncWorker
                // will call goOnline() / goOffline() on the next successful network
                // window (kick fires immediately; 15-min periodic fires as safety net).
                syncQueueDao.enqueue(
                    SyncQueueEntity(
                        action      = if (goingOnline) SyncAction.SHIFT_START else SyncAction.SHIFT_END,
                        payloadJson = "{}",   // action itself encodes intent; no extra fields needed
                        createdAt   = System.currentTimeMillis()
                    )
                )
                OutboundSyncWorker.kickOnce(context)
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
