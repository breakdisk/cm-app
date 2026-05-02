package io.logisticos.driver.feature.home.data

import io.logisticos.driver.core.database.dao.ShiftDao
import io.logisticos.driver.core.database.dao.TaskDao
import io.logisticos.driver.core.database.entity.ShiftEntity
import io.logisticos.driver.core.database.entity.TaskEntity
import io.logisticos.driver.core.database.entity.TaskStatus
import io.logisticos.driver.core.database.entity.TaskType
import io.logisticos.driver.core.network.service.DriverOpsApiService
import kotlinx.coroutines.flow.Flow
import javax.inject.Inject

class ShiftRepository @Inject constructor(
    private val api: DriverOpsApiService,
    private val shiftDao: ShiftDao,
    private val taskDao: TaskDao
) {
    fun observeActiveShift(): Flow<ShiftEntity?> = shiftDao.getActiveShift()

    /**
     * Fetches tasks from GET /v1/tasks and reconciles the local DB against
     * the authoritative server payload.
     *
     * The backend doesn't have a shift concept on this endpoint, so we use a
     * synthetic shift keyed by driver+date to satisfy the local schema.
     *
     * Reconciliation rules:
     *  - Server is the source of truth for *which* tasks belong to the driver
     *    today. Tasks the server omits (completed/cancelled/reassigned) are
     *    pruned from local Room — without this they linger forever and the
     *    route screen shows ghost stops.
     *  - Local-side mutations (status, stopOrder) survive a re-sync — the
     *    driver's in-flight work isn't clobbered by the server payload.
     *  - Empty payload is a valid state, not a no-op: the driver simply has
     *    no work today, and any leftover local tasks must go.
     */
    suspend fun syncShift() {
        val response = api.listMyTasks()
        val tasks = response.data

        // Use the device's local calendar date so the shift ID never rolls over
        // at midnight UTC (= 04:00 UAE / 08:00 PH). Previously dividing epoch ms
        // by 86_400_000 produced a UTC day number that ticked mid-morning for
        // drivers in UTC+4/+8 — after that tick Home showed tasks from the old
        // shift ID while Route loaded the new (empty) shift, producing the
        // "tasks visible on home, route blank" bug.
        val syntheticShiftId = "local-${java.time.LocalDate.now()}"
        val existingShift = shiftDao.getShiftById(syntheticShiftId)

        // Deactivate every other shift row before marking this one active.
        // Without this, stale rows from prior days linger with isActive=1 and
        // ShiftDao.getActiveShift() (no ORDER BY) picks one indeterminately.
        shiftDao.deactivateAllExcept(syntheticShiftId)

        // Always keep the shift row in sync with the server's task count, even
        // when the count is zero. Otherwise the home screen keeps reporting
        // yesterday's stop totals.
        shiftDao.insert(
            ShiftEntity(
                id = syntheticShiftId,
                driverId = "",
                tenantId = "",
                startedAt = existingShift?.startedAt,
                endedAt = null,
                isActive = tasks.isNotEmpty(),
                totalStops = tasks.size,
                completedStops = existingShift?.completedStops ?: 0,
                failedStops = existingShift?.failedStops ?: 0,
                totalCodCollected = existingShift?.totalCodCollected ?: 0.0,
                syncedAt = System.currentTimeMillis()
            )
        )

        if (tasks.isEmpty()) {
            taskDao.deleteForShift(syntheticShiftId)
            return
        }

        // Preserve locally-modified statuses — don't overwrite in-progress work
        val existingStatusMap = taskDao.getTasksForShiftOnce(syntheticShiftId).associateBy { it.id }

        val entities = tasks.map { t ->
            val existing = existingStatusMap[t.taskId]
            TaskEntity(
                id = t.taskId,
                shiftId = syntheticShiftId,
                shipmentId = t.shipmentId,
                taskType = when (t.taskType.lowercase()) {
                    "pickup"   -> TaskType.PICKUP
                    "return"   -> TaskType.RETURN
                    "hub_drop" -> TaskType.HUB_DROP
                    else       -> TaskType.DELIVERY
                },
                // Server now returns tracking_number for the AWB scan check.
                // Falls back to shipmentId so PickupScreen still shows
                // *something* in the "Expected" row instead of blank if a
                // legacy task pre-dates the migration.
                awb = t.trackingNumber ?: t.shipmentId,
                recipientName = t.customerName,
                recipientPhone = t.customerPhone,
                address = t.address,
                lat = t.lat ?: 0.0,
                lng = t.lng ?: 0.0,
                status = existing?.status ?: TaskStatus.ASSIGNED,
                stopOrder = existing?.stopOrder ?: t.sequence,
                requiresPhoto = t.requiresPhoto,
                requiresSignature = t.requiresSignature,
                requiresOtp = t.requiresOtp,
                isCod = (t.codAmountCents ?: 0L) > 0L,
                codAmount = (t.codAmountCents ?: 0L) / 100.0,
                syncedAt = System.currentTimeMillis()
            )
        }
        // Prune locally-known tasks that the server no longer reports for
        // this shift, then upsert the authoritative set. Order matters:
        // pruning before upsert is safe because insertAll is REPLACE-on-conflict.
        taskDao.pruneShiftTasks(syntheticShiftId, entities.map { it.id })
        taskDao.insertAll(entities)
    }
}
