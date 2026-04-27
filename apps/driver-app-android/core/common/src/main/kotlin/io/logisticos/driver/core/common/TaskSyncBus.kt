package io.logisticos.driver.core.common

import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.asSharedFlow

object TaskSyncBus {
    private val _events = MutableSharedFlow<Unit>(extraBufferCapacity = 1)
    val events: SharedFlow<Unit> = _events.asSharedFlow()

    /** Fire-and-forget: if no collector is active the event is buffered (capacity=1). */
    fun requestSync() {
        _events.tryEmit(Unit)
    }
}
