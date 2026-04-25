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
     * Fetches tasks from GET /v1/tasks and upserts into local DB.
     * The backend doesn't have a shift concept on this endpoint, so we use a
     * synthetic shift keyed by driver+date to satisfy the local schema.
     */
    suspend fun syncShift() {
        val response = api.listMyTasks()
        val tasks = response.data

        if (tasks.isEmpty()) return

        // Create / update a synthetic shift record so the shift UI still works
        val syntheticShiftId = "local-${System.currentTimeMillis() / 86_400_000}"
        val existingShift = shiftDao.getShiftById(syntheticShiftId)
        shiftDao.insert(
            ShiftEntity(
                id = syntheticShiftId,
                driverId = "",
                tenantId = "",
                startedAt = existingShift?.startedAt,
                endedAt = null,
                isActive = true,
                totalStops = tasks.size,
                completedStops = existingShift?.completedStops ?: 0,
                failedStops = existingShift?.failedStops ?: 0,
                totalCodCollected = existingShift?.totalCodCollected ?: 0.0,
                syncedAt = System.currentTimeMillis()
            )
        )

        // Preserve locally-modified statuses — don't overwrite in-progress work
        val existingStatusMap = taskDao.getTasksForShiftOnce(syntheticShiftId).associateBy { it.id }

        val entities = tasks.mapIndexed { idx, t ->
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
        taskDao.insertAll(entities)
    }
}
