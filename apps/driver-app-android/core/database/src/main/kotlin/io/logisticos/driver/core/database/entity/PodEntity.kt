package io.logisticos.driver.core.database.entity

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
    val lastSyncError: String? = null
)
