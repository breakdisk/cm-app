package io.logisticos.driver.core.database.entity

import androidx.room.Entity
import androidx.room.PrimaryKey

@Entity(tableName = "routes")
data class RouteEntity(
    @PrimaryKey val taskId: String,
    val polylineEncoded: String,
    val distanceMeters: Int,
    val durationSeconds: Int,
    val stepsJson: String,
    val etaTimestamp: Long,
    val fetchedAt: Long
)
