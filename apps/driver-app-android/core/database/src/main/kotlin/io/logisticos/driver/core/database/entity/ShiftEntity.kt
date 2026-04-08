package io.logisticos.driver.core.database.entity

import androidx.room.Entity
import androidx.room.PrimaryKey

@Entity(tableName = "shifts")
data class ShiftEntity(
    @PrimaryKey val id: String,
    val driverId: String,
    val tenantId: String,
    val startedAt: Long?,
    val endedAt: Long?,
    val isActive: Boolean,
    val totalStops: Int,
    val completedStops: Int,
    val failedStops: Int,
    val totalCodCollected: Double,
    val syncedAt: Long?
)
