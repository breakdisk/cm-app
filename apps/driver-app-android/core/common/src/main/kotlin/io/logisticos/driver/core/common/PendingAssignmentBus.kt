package io.logisticos.driver.core.common

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * Carries the FCM `task_assigned` payload across process boundaries
 * (DriverMessagingService → ShiftScaffold nav observer).
 *
 * Fields mirror what the backend sends in the FCM data map so the
 * AssignmentScreen can render without a separate network round-trip.
 */
data class AssignmentPayload(
    val assignmentId:   String,
    val shipmentId:     String,
    val customerName:   String,
    val address:        String,
    val taskType:       String,   // "pickup" | "delivery"
    val trackingNumber: String,
    val codAmountCents: Long,
)

object PendingAssignmentBus {
    // StateFlow<nullable>: null = no pending assignment, non-null = awaiting driver action.
    // StateFlow always replays the current value to new collectors, so an assignment that
    // arrives before ShiftScaffold enters composition is never dropped. Calling clear()
    // after the driver acts prevents stale re-delivery on recomposition / process restore.
    private val _pending = MutableStateFlow<AssignmentPayload?>(null)
    val pending: StateFlow<AssignmentPayload?> = _pending.asStateFlow()

    /** Called by DriverMessagingService on the worker thread — safe to call without coroutine. */
    fun post(payload: AssignmentPayload) {
        _pending.value = payload
    }

    /** Called by ShiftScaffold after the driver accepts or rejects an assignment. */
    fun clear() {
        _pending.value = null
    }
}
