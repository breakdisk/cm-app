package io.logisticos.driver.core.common

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * Carries the FCM `task_assigned` / `task_offer` payload across process
 * boundaries (DriverMessagingService → ShiftScaffold nav observer / Home).
 *
 * Fields mirror what the backend sends in the FCM data map so the
 * AssignmentScreen / grab card can render without a separate network round-trip.
 */
data class AssignmentPayload(
    val assignmentId:   String,
    val shipmentId:     String,
    val customerName:   String,
    val address:        String,
    val taskType:       String,   // "pickup" | "delivery"
    val trackingNumber: String,
    val codAmountCents: Long,
    /** Merchant / sender display name — task-card header. Empty when unknown. */
    val merchantName:   String = "",
    /** "food" | "parcel" | "grocery" | "medicine" | "heavy" | "large" — card icon. */
    val deliveryCategory: String = "parcel",
    /** Declared weight in grams (0 = unknown). */
    val weightGrams:    Long = 0L,
    /** Full route ends for the pickup→delivery sketch on the offer card. */
    val pickupLat:      Double? = null,
    val pickupLng:      Double? = null,
    val deliveryLat:    Double? = null,
    val deliveryLng:    Double? = null,
    // ── Broadcast gig offer ("grab") fields — blank/null for 1:1 assignments ──
    /** Non-blank = this is a contended broadcast offer rendered as a GRAB card. */
    val offerId:        String = "",
    /** THIS driver's contractual payout for the offer (cents). Null = hidden. */
    val payoutCents:    Long? = null,
    /** Offer TTL deadline (epoch millis) — drives the countdown ring. */
    val expiresAtMillis: Long? = null,
) {
    /** True when this payload is a contended broadcast offer (grab flow). */
    val isGrabOffer: Boolean get() = offerId.isNotBlank()
}

object PendingAssignmentBus {
    // StateFlow<nullable>: null = no pending assignment, non-null = awaiting driver action.
    // StateFlow always replays the current value to new collectors, so an assignment that
    // arrives before ShiftScaffold enters composition is never dropped. Calling clear()
    // after the driver acts prevents stale re-delivery on recomposition / process restore.
    private val _pending = MutableStateFlow<AssignmentPayload?>(null)
    val pending: StateFlow<AssignmentPayload?> = _pending.asStateFlow()

    // Offer id whose race was just lost ("Taken" flash). Set by the FCM
    // offer_closed handler or by a 409 on claim; the UI shows the taken state
    // briefly, then calls clearTaken() + clear().
    private val _takenOfferId = MutableStateFlow<String?>(null)
    val takenOfferId: StateFlow<String?> = _takenOfferId.asStateFlow()

    /** Called by DriverMessagingService on the worker thread — safe to call without coroutine. */
    fun post(payload: AssignmentPayload) {
        _pending.value = payload
    }

    /** Called by ShiftScaffold / Home after the driver acts on the payload. */
    fun clear() {
        _pending.value = null
    }

    /**
     * Another driver won this offer (FCM `offer_closed` or claim 409).
     * Flags the taken state only when the offer is still the one on screen.
     */
    fun markTaken(offerId: String) {
        if (_pending.value?.offerId == offerId) {
            _takenOfferId.value = offerId
        }
    }

    fun clearTaken() {
        _takenOfferId.value = null
    }
}
