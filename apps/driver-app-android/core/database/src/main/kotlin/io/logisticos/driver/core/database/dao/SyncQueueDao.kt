package io.logisticos.driver.core.database.dao

import androidx.room.*
import io.logisticos.driver.core.database.entity.SyncQueueEntity
import kotlinx.coroutines.flow.Flow

@Dao
interface SyncQueueDao {
    @Insert
    suspend fun enqueue(item: SyncQueueEntity): Long

    @Query("SELECT * FROM sync_queue WHERE nextRetryAt <= :now ORDER BY createdAt ASC LIMIT 50")
    suspend fun getPendingItems(now: Long): List<SyncQueueEntity>

    @Query("DELETE FROM sync_queue WHERE id = :id")
    suspend fun remove(id: Long)

    @Query("UPDATE sync_queue SET retryCount = retryCount + 1, lastError = :error, nextRetryAt = :nextRetry WHERE id = :id")
    suspend fun markFailed(id: Long, error: String, nextRetry: Long)

    /** Total items in queue (including those waiting for backoff). */
    @Query("SELECT COUNT(*) FROM sync_queue")
    fun getPendingCount(): Flow<Int>

    /** Items that are ready to upload NOW (not blocked by backoff delay).
     *  Use this for the driver-facing badge so it reflects real activity. */
    @Query("SELECT COUNT(*) FROM sync_queue WHERE nextRetryAt <= :now")
    fun getActivePendingCount(now: Long): Flow<Int>
}
