package io.logisticos.driver.core.common

import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.asSharedFlow

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
    private val _events = MutableSharedFlow<AssignmentPayload>(extraBufferCapacity = 1)
    val events: SharedFlow<AssignmentPayload> = _events.asSharedFlow()

    /** Called by DriverMessagingService on the worker thread — safe to call without coroutine. */
    fun post(payload: AssignmentPayload) {
        _events.tryEmit(payload)
    }
}
