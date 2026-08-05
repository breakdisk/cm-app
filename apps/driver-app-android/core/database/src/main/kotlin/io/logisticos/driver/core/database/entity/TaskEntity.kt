package io.logisticos.driver.core.database.entity

import androidx.room.ColumnInfo
import androidx.room.Entity
import androidx.room.PrimaryKey

enum class TaskStatus {
    ASSIGNED, EN_ROUTE, ARRIVED, IN_PROGRESS, COMPLETED, ATTEMPTED, FAILED, RETURNED,

    /**
     * Deprecated — no longer written. Sync failure is tracked by
     * [TaskEntity.syncFailed] instead.
     *
     * This value conflated two independent things: where the task is in its
     * lifecycle, and whether the backend has acknowledged it. Writing it
     * destroyed the fact that the task had reached COMPLETED, and because the
     * state machine treats it as terminal, every entry point silently refused to
     * act on the task afterwards — the driver had no way forward and the local
     * record no longer said the delivery had happened.
     *
     * Retained only so Room can still deserialize rows written by older builds;
     * MIGRATION_8_9 rewrites them. Remove once no such rows can remain.
     *
     * Not annotated `@Deprecated`: the few remaining references (the state-machine
     * table, the status→colour map, its test) are all deliberate old-row handling,
     * so the annotation would produce warnings without telling anyone anything.
     */
    FAILED_SYNC
}

enum class TaskType {
    PICKUP,      // First-mile: collect parcel from merchant
    DELIVERY,    // Last-mile: deliver parcel to recipient
    RETURN,      // Return undelivered parcel to hub
    HUB_DROP     // Drop parcels at sorting hub
}

@Entity(tableName = "tasks")
data class TaskEntity(
    @PrimaryKey val id: String,
    val shiftId: String = "",
    val shipmentId: String = "",            // UUID of the shipment — required for POD initiation
    val taskType: TaskType = TaskType.DELIVERY,
    val awb: String,
    val recipientName: String,
    val recipientPhone: String,
    val address: String,
    val lat: Double = 0.0,
    val lng: Double = 0.0,
    val status: TaskStatus,
    val stopOrder: Int,
    val requiresPhoto: Boolean = false,
    val requiresSignature: Boolean = false,
    val requiresOtp: Boolean = false,
    val isCod: Boolean = false,
    val codAmount: Double = 0.0,
    val attemptCount: Int = 0,
    val failureReason: String? = null,
    val notes: String? = null,
    val syncedAt: Long?,
    val isSynced: Boolean = true,
    val podId: String? = null,
    @ColumnInfo(name = "pop_id") val popId: String? = null,
    val completedAt: Long? = null,
    /**
     * True when the task's evidence exhausted its sync retry window and was
     * abandoned by [io.logisticos.driver.core.database.worker.OutboundSyncWorker].
     *
     * Deliberately separate from [status]: the task itself did complete, only the
     * hand-off to the backend failed. Keeping them apart preserves the local
     * record of what actually happened and leaves the task eligible for a retry
     * that re-enqueues the sync item.
     */
    @ColumnInfo(name = "sync_failed") val syncFailed: Boolean = false,
)
