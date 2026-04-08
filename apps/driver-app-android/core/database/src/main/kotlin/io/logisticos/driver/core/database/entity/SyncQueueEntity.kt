package io.logisticos.driver.core.database.entity

import androidx.room.Entity
import androidx.room.PrimaryKey

enum class SyncAction {
    TASK_STATUS_UPDATE, POD_SUBMIT, SCAN_EVENT, COD_CONFIRM, SHIFT_START, SHIFT_END
}

@Entity(tableName = "sync_queue")
data class SyncQueueEntity(
    @PrimaryKey(autoGenerate = true) val id: Long = 0,
    val action: SyncAction,
    val payloadJson: String,
    val createdAt: Long,
    val retryCount: Int = 0,
    val lastError: String? = null,
    val nextRetryAt: Long = 0
)
