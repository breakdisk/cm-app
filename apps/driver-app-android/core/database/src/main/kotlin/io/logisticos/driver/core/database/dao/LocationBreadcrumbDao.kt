package io.logisticos.driver.core.database.dao

import androidx.room.*
import io.logisticos.driver.core.database.entity.LocationBreadcrumbEntity

@Dao
interface LocationBreadcrumbDao {
    @Insert
    suspend fun insert(breadcrumb: LocationBreadcrumbEntity)

    @Query("SELECT * FROM location_breadcrumbs WHERE isSynced = 0 LIMIT 200")
    suspend fun getUnsynced(): List<LocationBreadcrumbEntity>

    @Query("UPDATE location_breadcrumbs SET isSynced = 1 WHERE id IN (:ids)")
    suspend fun markSynced(ids: List<Long>)

    @Query("DELETE FROM location_breadcrumbs WHERE isSynced = 1 AND timestamp < :olderThan")
    suspend fun pruneOld(olderThan: Long)
}
