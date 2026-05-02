package io.logisticos.driver.core.database.dao

import androidx.room.*
import io.logisticos.driver.core.database.entity.ShiftEntity
import kotlinx.coroutines.flow.Flow

@Dao
interface ShiftDao {
    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun insert(shift: ShiftEntity)

    @Query("SELECT * FROM shifts WHERE isActive = 1 LIMIT 1")
    fun getActiveShift(): Flow<ShiftEntity?>

    @Query("SELECT * FROM shifts WHERE isActive = 1 LIMIT 1")
    suspend fun getActiveShiftOnce(): ShiftEntity?

    @Query("UPDATE shifts SET isActive = 0, endedAt = :endedAt WHERE id = :shiftId")
    suspend fun endShift(shiftId: String, endedAt: Long)

    /** Deactivate every shift row except the given one — prevents stale rows from
     *  polluting getActiveShift() which has no ORDER BY and picks indeterminately. */
    @Query("UPDATE shifts SET isActive = 0 WHERE id != :keepId")
    suspend fun deactivateAllExcept(keepId: String)

    @Query("UPDATE shifts SET completedStops = completedStops + 1 WHERE id = :shiftId")
    suspend fun incrementCompleted(shiftId: String)

    @Query("UPDATE shifts SET failedStops = failedStops + 1 WHERE id = :shiftId")
    suspend fun incrementFailed(shiftId: String)

    @Query("UPDATE shifts SET totalCodCollected = totalCodCollected + :amount WHERE id = :shiftId")
    suspend fun addCodCollected(shiftId: String, amount: Double)

    @Query("SELECT * FROM shifts WHERE id = :shiftId LIMIT 1")
    suspend fun getShiftById(shiftId: String): ShiftEntity?
}
