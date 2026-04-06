package io.logisticos.driver.core.database.dao

import androidx.room.*
import io.logisticos.driver.core.database.entity.PodEntity

@Dao
interface PodDao {
    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun insert(pod: PodEntity)

    @Query("SELECT * FROM pod WHERE taskId = :taskId")
    suspend fun getForTask(taskId: String): PodEntity?

    @Query("SELECT * FROM pod WHERE isSynced = 0")
    suspend fun getUnsynced(): List<PodEntity>

    @Query("UPDATE pod SET isSynced = 1 WHERE taskId = :taskId")
    suspend fun markSynced(taskId: String)

    @Query("UPDATE pod SET syncAttempts = syncAttempts + 1, lastSyncError = :error WHERE taskId = :taskId")
    suspend fun markSyncFailed(taskId: String, error: String)
}
