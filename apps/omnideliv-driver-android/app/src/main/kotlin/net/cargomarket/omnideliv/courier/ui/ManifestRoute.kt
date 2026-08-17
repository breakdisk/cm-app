package net.cargomarket.omnideliv.courier.ui

import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import net.cargomarket.omnideliv.courier.data.CourierApi
import net.cargomarket.omnideliv.courier.data.OutboundRepository
import net.cargomarket.omnideliv.courier.data.DropoffDto
import net.cargomarket.omnideliv.courier.data.LineDto
import net.cargomarket.omnideliv.courier.data.ManifestDto
import net.cargomarket.omnideliv.courier.data.StopDto
import net.cargomarket.omnideliv.courier.domain.Dropoff
import net.cargomarket.omnideliv.courier.domain.GeofenceAdvice
import net.cargomarket.omnideliv.courier.domain.Leg
import net.cargomarket.omnideliv.courier.domain.Line
import net.cargomarket.omnideliv.courier.domain.MilestoneKind
import net.cargomarket.omnideliv.courier.domain.Manifest
import net.cargomarket.omnideliv.courier.domain.Stop
import net.cargomarket.omnideliv.courier.domain.currentLeg
import javax.inject.Inject
import kotlin.coroutines.coroutineContext

/**
 * The stateful half of the manifest.
 *
 * [ManifestScreen] stays a pure function of a [Manifest], so it can be previewed
 * and reasoned about without navigation or a network. This wrapper is what knows
 * about order ids, polling and failure.
 */

/** DTO to domain. Hand-written so the wire shape can change without the UI moving. */
internal fun ManifestDto.toDomain() = Manifest(
    orderId = orderId,
    status = status,
    codAmountCents = codAmountCents,
    tripCents = tripCents,
    tipCents = tipCents,
    stops = stops.map(StopDto::toDomain),
    dropoff = dropoff.toDomain(),
)

internal fun StopDto.toDomain() = Stop(
    stopRef = stopRef,
    seq = seq,
    vendorName = vendorName,
    address = address,
    lat = lat,
    lng = lng,
    vertical = vertical,
    prepTimeMinutes = prepTimeMinutes,
    pickedUp = pickedUp,
    lines = lines.map(LineDto::toDomain),
)

internal fun LineDto.toDomain() = Line(qty = qty, itemName = itemName, modifiers = modifiers)

internal fun DropoffDto.toDomain() = Dropoff(
    stopRef = stopRef,
    lat = lat,
    lng = lng,
    customerName = customerName,
    customerPhone = customerPhone,
    notes = notes,
)

data class ManifestUiState(
    val manifest: Manifest? = null,
    /**
     * True once a fetch has failed and the manifest on screen is the last one
     * that arrived. Drives the render-cache indicator — a stale route shown as
     * live is how a courier drives to a stop that was re-sequenced.
     */
    val servedFromCache: Boolean = false,
    val loading: Boolean = true,
    /** Milestones recorded locally that the server has not accepted yet. */
    val pendingCount: Int = 0,
)

@HiltViewModel
class ManifestViewModel @Inject constructor(
    private val api: CourierApi,
    private val outbound: OutboundRepository,
) : ViewModel() {

    private val _state = MutableStateFlow(ManifestUiState())

    /**
     * The manifest, with the pending count folded in.
     *
     * Combined rather than stored: the count belongs to the queue, and copying
     * it into this state would give two places that could disagree about how
     * much work is unsynced.
     */
    val state: StateFlow<ManifestUiState> =
        combine(_state, outbound.pendingCount) { s, pending -> s.copy(pendingCount = pending) }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), ManifestUiState())

    private var started = false

    /**
     * Begin polling for one order.
     *
     * Guarded against restarting so a recomposition does not start a second
     * loop. The manifest is re-read rather than cached as truth, because §2 of
     * the specification has the agent rewriting a route mid-job — a cached
     * manifest would let a courier keep working a route that no longer exists.
     */
    fun start(orderId: String) {
        if (started || orderId.isBlank()) return
        started = true
        viewModelScope.launch {
            while (coroutineContext.isActive) {
                fetch(orderId)
                delay(POLL_MS)
            }
        }
    }

    private suspend fun fetch(orderId: String) {
        val result = runCatching { api.manifest(orderId) }
        result.fold(
            onSuccess = { res ->
                val body = res.body()
                _state.value = if (res.isSuccessful && body != null) {
                    ManifestUiState(body.toDomain(), servedFromCache = false, loading = false)
                } else {
                    // Keep what is on screen. A courier mid-route needs the stop
                    // they are driving to more than they need an error page.
                    _state.value.copy(servedFromCache = true, loading = false)
                }
            },
            onFailure = {
                _state.value = _state.value.copy(servedFromCache = true, loading = false)
            },
        )
    }

    /**
     * Record the next milestone for whatever the courier is doing.
     *
     * Enqueued, never sent directly. The courier moves on immediately and the
     * queue is what gets it to the server — which is the whole reason a delivery
     * can be finished in a basement. The optimistic refresh afterwards is a
     * courtesy; the record is already safe on disk.
     *
     * The device timestamp is taken inside `record`, at this instant, because
     * this call *is* the physical event.
     */
    fun advance(assignmentId: String) {
        val manifest = _state.value.manifest ?: return
        val (kind, stopRef) = when (val leg = manifest.currentLeg()) {
            is Leg.ToPickup -> MilestoneKind.COLLECTED to leg.stop.stopRef
            is Leg.ToDropoff -> MilestoneKind.DELIVERED to leg.dropoff.stopRef
            Leg.Done -> return
        }

        viewModelScope.launch {
            outbound.record(kind, assignmentId, stopRef)
            // Best-effort. A failure here changes nothing: the row is on disk
            // and the worker will carry it.
            runCatching { outbound.drain() }
            // Re-read so a leg the server accepted stops showing as outstanding
            // without waiting out the poll interval.
            _state.value.manifest?.orderId?.let { fetch(it) }
        }
    }

    private companion object {
        /**
         * Ten seconds while the screen is open. The spec's adaptive schedule
         * (10s near a stop, 30s otherwise, none in the background) needs the
         * location service to say how near "near" is; until that is wired this
         * is the shorter of the two, which is the safe direction — it costs
         * data, not correctness.
         */
        const val POLL_MS = 10_000L
    }
}

/**
 * Named `ManifestRoute`, not a second `ManifestScreen` overload.
 *
 * Two same-named composables in one package differing only by parameter list is
 * a resolution bug waiting for someone to add a default argument.
 */
@Composable
fun ManifestRoute(
    orderId: String,
    assignmentId: String,
    vm: ManifestViewModel = hiltViewModel(),
) {
    val state by vm.state.collectAsState()

    // In an effect rather than in composition: composition can run many times
    // and must stay free of side effects. Keyed on the order, which is fixed for
    // this screen's lifetime, so the poll starts once.
    LaunchedEffect(orderId) { vm.start(orderId) }

    ManifestScreen(
        manifest = state.manifest,
        advice = GeofenceAdvice.NoFix,
        servedFromCache = state.servedFromCache,
        pendingCount = state.pendingCount,
        onAdvance = { vm.advance(assignmentId) },
        onIssue = { /* the exception flow is its own spec */ },
    )
}
