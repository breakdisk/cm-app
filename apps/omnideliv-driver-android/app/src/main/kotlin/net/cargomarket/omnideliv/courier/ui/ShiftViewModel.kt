package net.cargomarket.omnideliv.courier.ui

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import net.cargomarket.omnideliv.courier.data.CourierApi
import net.cargomarket.omnideliv.courier.data.OfferDto
import net.cargomarket.omnideliv.courier.data.SetStatusRequest
import net.cargomarket.omnideliv.courier.domain.OfferCard
import net.cargomarket.omnideliv.courier.domain.parseOfferCard
import javax.inject.Inject
import kotlin.coroutines.coroutineContext

/** One offer, with its card already parsed. */
data class OfferRow(
    val assignmentId: String,
    val externalRef: String,
    val tripCents: Long,
    val tipCents: Long,
    val codAmountCents: Long,
    /** `null` when the product sent no card, or one this build cannot read. */
    val card: OfferCard?,
) {
    /** What the courier earns. Never includes the cash — that is not theirs. */
    fun earningsCents(): Long = tripCents + tipCents
}

sealed interface ShiftState {
    /**
     * Off duty. Nothing is polled and no offers arrive.
     *
     * [notice] carries why, when this is the result of a failed attempt to go
     * on duty rather than a courier's own choice.
     */
    data class Offline(val notice: String? = null) : ShiftState

    /** On duty, listing whatever is currently offered. */
    data class Online(
        val offers: List<OfferRow> = emptyList(),
        val claiming: String? = null,
        /** Set when the last poll failed, cleared by the next success. */
        val stale: Boolean = false,
        val notice: String? = null,
    ) : ShiftState

    /** A job was claimed. The host navigates to the manifest. */
    data class Claimed(val externalRef: String, val assignmentId: String) : ShiftState
}

@HiltViewModel
class ShiftViewModel @Inject constructor(
    private val api: CourierApi,
) : ViewModel() {

    private val _state = MutableStateFlow<ShiftState>(ShiftState.Offline())
    val state: StateFlow<ShiftState> = _state.asStateFlow()

    private var poller: Job? = null

    /**
     * A go-on-duty request is in flight.
     *
     * The state is still `Offline` while the server is being asked, so the
     * state check alone would let a second tap start a second request and a
     * second poller.
     */
    private var goingOnDuty = false

    /**
     * Go on duty.
     *
     * The server is told first, and the screen only says "on duty" once it has
     * agreed. This app shipped with a toggle that set a local flag and nothing
     * else: field-ops kept the `offline` that registration gave the courier,
     * `find_available_near` skipped them, and every order fanned out to nobody
     * while the phone read *Watching for offers*. A toggle that lies about this
     * is worse than no toggle, because the courier waits on it.
     *
     * Polling starts here rather than at app launch, so a courier who opened
     * the app to check their earnings is not offered work and does not spend
     * battery or data on a shift they have not started.
     */
    fun goOnline() {
        if (_state.value !is ShiftState.Offline || goingOnDuty) return
        goingOnDuty = true

        viewModelScope.launch {
            val accepted = runCatching { api.setStatus(SetStatusRequest(available = true)) }
                .getOrNull()
                ?.isSuccessful == true
            goingOnDuty = false

            if (!accepted) {
                // Stay off duty, and say so. Anything else strands the courier
                // on an empty list that can never fill.
                _state.value = ShiftState.Offline(
                    "Could not go on duty. Check your connection and try again.",
                )
                return@launch
            }

            _state.value = ShiftState.Online()
            poller = viewModelScope.launch {
                while (coroutineContext.isActive) {
                    refresh()
                    delay(POLL_MS)
                }
            }
        }
    }

    /**
     * Go off duty.
     *
     * Optimistic, unlike going on: a courier who has finished must not be held
     * on shift by a bad signal. If the call fails they stay `available` on the
     * server for a while, but the location service stops with the shift and
     * `find_available_near` ignores any fix older than ten minutes — so supply
     * self-heals within that window without the app having to retry.
     */
    fun goOffline() {
        stopPolling()
        _state.value = ShiftState.Offline()
        viewModelScope.launch {
            runCatching { api.setStatus(SetStatusRequest(available = false)) }
        }
    }

    /**
     * Stop listening for offers without reporting off duty.
     *
     * The distinction is the point: a courier who just claimed a job is not
     * shopping for another, but they are very much working. Reporting them
     * offline here would take them out of supply for good the moment the job
     * ended.
     */
    private fun stopPolling() {
        poller?.cancel()
        poller = null
    }

    private suspend fun refresh() {
        val current = _state.value as? ShiftState.Online ?: return
        val result = runCatching { api.myOffers() }

        result.fold(
            onSuccess = { res ->
                val body = res.body()
                if (res.isSuccessful && body != null) {
                    _state.value = current.copy(
                        offers = body.offers.map(::toRow),
                        stale = false,
                    )
                } else {
                    // Keep whatever is on screen and mark it stale. Blanking the
                    // list on one bad poll would make a job the courier is
                    // reading vanish under them.
                    _state.value = current.copy(stale = true)
                }
            },
            onFailure = { _state.value = current.copy(stale = true) },
        )
    }

    /**
     * Take a job.
     *
     * A lost race is a normal outcome, not an error: field-ops answers
     * `{won:false}` for both "someone else got it" and "not yours", deliberately
     * indistinguishable so ids cannot be probed. Either way the courier is told
     * it is gone and the list refreshes.
     */
    fun claim(assignmentId: String) {
        val current = _state.value as? ShiftState.Online ?: return
        if (current.claiming != null) return
        _state.value = current.copy(claiming = assignmentId, notice = null)

        viewModelScope.launch {
            val row = current.offers.firstOrNull { it.assignmentId == assignmentId }
            val result = runCatching { api.claim(assignmentId) }

            result.fold(
                onSuccess = { res ->
                    val won = res.body()?.won == true
                    if (res.isSuccessful && won && row != null) {
                        stopPolling()
                        _state.value = ShiftState.Claimed(row.externalRef, row.assignmentId)
                    } else {
                        val after = _state.value as? ShiftState.Online ?: return@fold
                        _state.value = after.copy(
                            claiming = null,
                            notice = if (res.isSuccessful) {
                                "That job was taken."
                            } else {
                                "Could not take that job. Try again."
                            },
                        )
                        refresh()
                    }
                },
                onFailure = {
                    val after = _state.value as? ShiftState.Online ?: return@fold
                    _state.value = after.copy(
                        claiming = null,
                        notice = "No signal. Could not take that job.",
                    )
                },
            )
        }
    }

    private fun toRow(dto: OfferDto) = OfferRow(
        assignmentId = dto.assignmentId,
        externalRef = dto.externalRef,
        tripCents = dto.tripCents,
        tipCents = dto.tipCents,
        codAmountCents = dto.codAmountCents,
        card = parseOfferCard(dto.offerCard),
    )

    override fun onCleared() {
        stopPolling()
        super.onCleared()
    }

    private companion object {
        /**
         * Offers expire, so a slow list is a list of jobs already gone. Six
         * seconds is the cadence the existing driver app settled on for the same
         * problem after FCM proved unreliable in the field.
         */
        const val POLL_MS = 6_000L
    }
}
