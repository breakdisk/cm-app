package io.logisticos.driver.core.database.entity

import androidx.room.Entity
import androidx.room.PrimaryKey

@Entity(tableName = "scan_events")
data class ScanEventEntity(
    @PrimaryKey(autoGenerate = true) val id: Long = 0,
    val taskId: String,
    val awb: String,
    val scannedAt: Long,
    val isSynced: Boolean = false
)
