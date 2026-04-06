package io.logisticos.driver.core.database.entity

import androidx.room.Entity
import androidx.room.PrimaryKey

@Entity(tableName = "location_breadcrumbs")
data class LocationBreadcrumbEntity(
    @PrimaryKey(autoGenerate = true) val id: Long = 0,
    val shiftId: String,
    val lat: Double,
    val lng: Double,
    val accuracy: Float,
    val speedMps: Float,
    val bearing: Float,
    val timestamp: Long,
    val isSynced: Boolean = false
)
