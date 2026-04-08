package io.logisticos.driver.core.database.dao

import androidx.room.*
import io.logisticos.driver.core.database.entity.ScanEventEntity

@Dao
interface ScanEventDao {
    @Insert
    suspend fun insert(event: ScanEventEntity)

    @Query("SELECT * FROM scan_events WHERE isSynced = 0")
    suspend fun getUnsynced(): List<ScanEventEntity>

    @Query("UPDATE scan_events SET isSynced = 1 WHERE id IN (:ids)")
    suspend fun markSynced(ids: List<Long>)
}
