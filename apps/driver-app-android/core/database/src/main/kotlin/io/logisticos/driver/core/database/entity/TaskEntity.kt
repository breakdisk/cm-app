package io.logisticos.driver.core.database.entity

import androidx.room.Entity
import androidx.room.PrimaryKey

enum class TaskStatus {
    ASSIGNED, EN_ROUTE, ARRIVED, IN_PROGRESS, COMPLETED, ATTEMPTED, FAILED, RETURNED
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
    val shiftId: String,
    val taskType: TaskType = TaskType.DELIVERY,
    val awb: String,
    val recipientName: String,
    val recipientPhone: String,
    val address: String,
    val lat: Double,
    val lng: Double,
    val status: TaskStatus,
    val stopOrder: Int,
    val requiresPhoto: Boolean,
    val requiresSignature: Boolean,
    val requiresOtp: Boolean,
    val isCod: Boolean,
    val codAmount: Double,
    val attemptCount: Int = 0,
    val failureReason: String? = null,
    val notes: String? = null,
    val syncedAt: Long?
)
