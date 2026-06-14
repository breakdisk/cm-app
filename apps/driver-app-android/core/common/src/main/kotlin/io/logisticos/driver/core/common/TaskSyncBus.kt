package io.logisticos.driver.core.common

import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow

object TaskSyncBus {
    private val _events = MutableSharedFlow<Unit>(extraBufferCapacity = 1)
    val events: SharedFlow<Unit> = _events.asSharedFlow()

    /** Fire-and-forget: if no collector is active the event is buffered (capacity=1). */
    fun requestSync() {
        _events.tryEmit(Unit)
    }

    // ── Optimistic local completion ──────────────────────────────────────────
    // When the pickup flow marks a task complete locally (Room), the next
    // API poll may still return it as IN_PROGRESS until the server catches up.
    // Tracking completed IDs here lets HomeViewModel filter them out of the
    // task card list immediately, so the card disappears on the driver's screen
    // without waiting for the next successful server round-trip.

    private val _locallyCompletedIds = MutableStateFlow<Set<String>>(emptySet())
    val locallyCompletedIds: StateFlow<Set<String>> = _locallyCompletedIds.asStateFlow()

    fun markLocallyCompleted(taskId: String) {
        _locallyCompletedIds.value = _locallyCompletedIds.value + taskId
    }

    /** Called once the server confirms the task is no longer in the active list. */
    fun clearLocallyCompleted(taskId: String) {
        _locallyCompletedIds.value = _locallyCompletedIds.value - taskId
    }
}
