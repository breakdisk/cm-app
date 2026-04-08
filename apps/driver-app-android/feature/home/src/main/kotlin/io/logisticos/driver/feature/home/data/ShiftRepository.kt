package io.logisticos.driver.feature.home.data

import io.logisticos.driver.core.database.dao.ShiftDao
import io.logisticos.driver.core.database.dao.TaskDao
import io.logisticos.driver.core.database.entity.ShiftEntity
import io.logisticos.driver.core.database.entity.TaskEntity
import io.logisticos.driver.core.database.entity.TaskStatus
import io.logisticos.driver.core.network.service.DriverOpsApiService
import kotlinx.coroutines.flow.Flow
import javax.inject.Inject

class ShiftRepository @Inject constructor(
    private val api: DriverOpsApiService,
    private val shiftDao: ShiftDao,
    private val taskDao: TaskDao
) {
    fun observeActiveShift(): Flow<ShiftEntity?> = shiftDao.getActiveShift()

    suspend fun syncShift() {
        val response = api.getActiveShift()

        // Preserve existing shift timestamps and counters
        val existingShift = shiftDao.getShiftById(response.id)
        shiftDao.insert(
            ShiftEntity(
                id = response.id,
                driverId = response.driverId,
                tenantId = response.tenantId,
                startedAt = existingShift?.startedAt,
                endedAt = existingShift?.endedAt,
                isActive = true,
                totalStops = response.totalStops,
                completedStops = existingShift?.completedStops ?: 0,
                failedStops = existingShift?.failedStops ?: 0,
                totalCodCollected = existingShift?.totalCodCollected ?: 0.0,
                syncedAt = System.currentTimeMillis()
            )
        )

        // Preserve existing task statuses so locally-modified states are not overwritten
        val existingTasks = taskDao.getTasksForShiftOnce(response.id)
        val existingStatusMap = existingTasks.associateBy { it.id }

        val tasks = response.tasks.map { t ->
            val existingTask = existingStatusMap[t.id]
            TaskEntity(
                id = t.id,
                shiftId = response.id,
                awb = t.awb,
                recipientName = t.recipientName,
                recipientPhone = t.recipientPhone,
                address = t.address,
                lat = t.lat,
                lng = t.lng,
                status = existingTask?.status ?: TaskStatus.ASSIGNED,
                stopOrder = existingTask?.stopOrder ?: t.stopOrder,
                requiresPhoto = t.requiresPhoto,
                requiresSignature = t.requiresSignature,
                requiresOtp = t.requiresOtp,
                isCod = t.isCod,
                codAmount = t.codAmount,
                notes = t.notes,
                syncedAt = System.currentTimeMillis()
            )
        }
        taskDao.insertAll(tasks)
    }
}
