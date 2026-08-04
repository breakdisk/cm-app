package io.logisticos.driver.core.database.entity

import androidx.room.ColumnInfo
import androidx.room.Entity
import androidx.room.PrimaryKey

@Entity(tableName = "pod")
data class PodEntity(
    @PrimaryKey val taskId: String,
    val photoPath: String?,
    val signaturePath: String?,
    val otpToken: String?,
    val capturedAt: Long,
    val isSynced: Boolean = false,
    val syncAttempts: Int = 0,
    val lastSyncError: String? = null,
    /**
     * Driver's actual GPS at the moment the POD was captured.
     *
     * Persisted because the offline replay path in `OutboundSyncWorker` runs
     * minutes-to-hours after the fact, by which time the device has moved. Without
     * this the worker had no capture position and fell back to the delivery
     * address, which made the server-side geofence a self-comparison that always
     * measured 0 m. Null only for POD rows written before this column existed.
     */
    @ColumnInfo(name = "capture_lat") val captureLat: Double? = null,
    @ColumnInfo(name = "capture_lng") val captureLng: Double? = null,
    /**
     * ISO-8601 UTC hardware-clock reading taken at the physical POD event, not at
     * sync time. Primary time basis for SLA calculations per the dual-timestamp
     * contract; `capturedAt` remains the local millis value used for ordering.
     */
    @ColumnInfo(name = "device_timestamp") val deviceTimestamp: String? = null,
)
