package io.logisticos.driver.core.database.dao

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import io.logisticos.driver.core.database.entity.NotificationEntity
import kotlinx.coroutines.flow.Flow

@Dao
interface NotificationDao {

    /** Upsert a notification — REPLACE handles the rare duplicate-id edge case. */
    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun insert(notification: NotificationEntity)

    /** All notifications newest-first; emits on every change (Room-reactive). */
    @Query("SELECT * FROM notifications ORDER BY receivedAt DESC")
    fun getAll(): Flow<List<NotificationEntity>>

    /** Mark every unread notification as read in a single UPDATE pass. */
    @Query("UPDATE notifications SET isRead = 1")
    suspend fun markAllRead()

    /**
     * Prune to the 100 most-recent notifications so the table doesn't grow
     * unbounded over a long-running shift.  Safe to call after every insert.
     */
    @Query("""
        DELETE FROM notifications
        WHERE id NOT IN (
            SELECT id FROM notifications ORDER BY receivedAt DESC LIMIT 100
        )
    """)
    suspend fun pruneOld()
}
