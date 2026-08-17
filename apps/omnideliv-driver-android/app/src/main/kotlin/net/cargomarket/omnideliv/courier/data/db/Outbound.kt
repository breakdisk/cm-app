package net.cargomarket.omnideliv.courier.data.db

import androidx.room.Dao
import androidx.room.Database
import androidx.room.Entity
import androidx.room.Insert
import androidx.room.PrimaryKey
import androidx.room.Query
import androidx.room.RoomDatabase
import kotlinx.coroutines.flow.Flow
import net.cargomarket.omnideliv.courier.domain.MilestoneKind
import net.cargomarket.omnideliv.courier.domain.QueuedMilestone
import net.cargomarket.omnideliv.courier.domain.SyncState

/**
 * The outbound queue.
 *
 * The only place in this app where a local write is authoritative-pending. The
 * manifest cache is a render cache that the server may overwrite at will; a row
 * in here is a claim the courier made that the server has not yet accepted, and
 * losing one loses a delivery.
 *
 * Room-backed rather than in-memory precisely so it survives process death:
 * WorkManager restarting after a low-memory kill mid-shift is the normal case,
 * not the exception.
 */
@Entity(tableName = "outbound")
data class OutboundEntity(
    @PrimaryKey(autoGenerate = true) val id: Long = 0,
    val kind: String,
    val assignmentId: String,
    val stopRef: String?,
    /**
     * Captured at the physical event, stored as ISO-8601 UTC, never re-read at
     * upload time. A payload that waited twenty minutes in a dead zone must not
     * bill those minutes to the courier's SLA.
     */
    val deviceTimestamp: String,
    val proofPath: String?,
    val attempts: Int = 0,
    val state: String = "PENDING",
    /** Why a row was parked, for the support surface. Null unless parked. */
    val parkedReason: String? = null,
)

internal fun OutboundEntity.toDomain() = QueuedMilestone(
    id = id,
    kind = MilestoneKind.valueOf(kind),
    assignmentId = assignmentId,
    stopRef = stopRef,
    deviceTimestamp = deviceTimestamp,
    proofPath = proofPath,
    attempts = attempts,
    state = SyncState.valueOf(state),
)

@Dao
interface OutboundDao {
    @Insert
    suspend fun insert(row: OutboundEntity): Long

    /**
     * Everything not yet accepted, oldest physical event first.
     *
     * Ordered by the device clock rather than by id: two milestones recorded
     * offline and inserted in whatever order the UI happened to write them must
     * still reach the server in the order they physically happened, or a
     * delivery can arrive before its own collection.
     */
    @Query("SELECT * FROM outbound WHERE state = 'PENDING' ORDER BY deviceTimestamp ASC, id ASC")
    suspend fun pending(): List<OutboundEntity>

    /** Drives the pending badge. A count, not a spinner — nothing is in progress offline. */
    @Query("SELECT COUNT(*) FROM outbound WHERE state = 'PENDING'")
    fun pendingCount(): Flow<Int>

    @Query("SELECT * FROM outbound WHERE state = 'PARKED'")
    suspend fun parked(): List<OutboundEntity>

    @Query("UPDATE outbound SET state = 'SYNCED' WHERE id = :id")
    suspend fun markSynced(id: Long)

    @Query("UPDATE outbound SET attempts = :attempts WHERE id = :id")
    suspend fun recordAttempt(id: Long, attempts: Int)

    @Query("UPDATE outbound SET state = 'PARKED', parkedReason = :reason WHERE id = :id")
    suspend fun park(id: Long, reason: String)
}

@Database(entities = [OutboundEntity::class], version = 1, exportSchema = false)
abstract class CourierDb : RoomDatabase() {
    abstract fun outbound(): OutboundDao
}
