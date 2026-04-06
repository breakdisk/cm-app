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
        shiftDao.insert(
            ShiftEntity(
                id = response.id,
                driverId = response.driverId,
                tenantId = response.tenantId,
                startedAt = null,
                endedAt = null,
                isActive = true,
                totalStops = response.totalStops,
                completedStops = 0,
                failedStops = 0,
                totalCodCollected = 0.0,
                syncedAt = System.currentTimeMillis()
            )
        )
        val tasks = response.tasks.map { t ->
            TaskEntity(
                id = t.id,
                shiftId = response.id,
                awb = t.awb,
                recipientName = t.recipientName,
                recipientPhone = t.recipientPhone,
                address = t.address,
                lat = t.lat,
                lng = t.lng,
                status = TaskStatus.ASSIGNED,
                stopOrder = t.stopOrder,
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
