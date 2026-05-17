package io.logisticos.driver.core.database.entity

import androidx.room.Entity
import androidx.room.PrimaryKey

/**
 * Persisted notification row. Backed by Room so notifications survive process death
 * and are visible after app restarts — previously the in-memory StateFlow cleared
 * on every cold start, making the notification bell always empty on relaunch.
 */
@Entity(tableName = "notifications")
data class NotificationEntity(
    @PrimaryKey val id: String,
    val type: String,
    val title: String,
    val body: String,
    val receivedAt: Long,
    val isRead: Boolean = false
)
