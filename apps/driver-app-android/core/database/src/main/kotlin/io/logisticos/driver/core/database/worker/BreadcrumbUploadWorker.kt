package io.logisticos.driver.core.database.worker

import android.content.Context
import androidx.hilt.work.HiltWorker
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.NetworkType
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import dagger.assisted.Assisted
import dagger.assisted.AssistedInject
import io.logisticos.driver.core.database.dao.LocationBreadcrumbDao
import io.logisticos.driver.core.database.dao.ShiftDao
import io.logisticos.driver.core.network.service.BreadcrumbBatchRequest
import io.logisticos.driver.core.network.service.BreadcrumbPoint
import io.logisticos.driver.core.network.service.TrackingApiService
import java.util.concurrent.TimeUnit

@HiltWorker
class BreadcrumbUploadWorker @AssistedInject constructor(
    @Assisted context: Context,
    @Assisted workerParams: WorkerParameters,
    private val breadcrumbDao: LocationBreadcrumbDao,
    private val shiftDao: ShiftDao,
    private val trackingApi: TrackingApiService
) : CoroutineWorker(context, workerParams) {

    override suspend fun doWork(): Result {
        val shift = shiftDao.getActiveShiftOnce() ?: return Result.success()
        val unsynced = breadcrumbDao.getUnsynced()
        if (unsynced.isEmpty()) return Result.success()

        return try {
            val response = trackingApi.uploadBreadcrumbs(
                BreadcrumbBatchRequest(
                    shiftId = shift.id,
                    points = unsynced.map { crumb ->
                        BreadcrumbPoint(
                            lat = crumb.lat,
                            lng = crumb.lng,
                            accuracy = crumb.accuracy,
                            speedMps = crumb.speedMps,
                            bearing = crumb.bearing,
                            timestamp = crumb.timestamp
                        )
                    }
                )
            )
            if (response.isSuccessful) {
                breadcrumbDao.markSynced(unsynced.map { it.id })
                breadcrumbDao.pruneOld(System.currentTimeMillis() - 24 * 60 * 60 * 1000L)
            }
            Result.success()
        } catch (e: Exception) {
            Result.success() // Periodic worker — will retry next period
        }
    }

    companion object {
        const val WORK_NAME = "breadcrumb_upload"

        fun schedule(context: Context) {
            val request = PeriodicWorkRequestBuilder<BreadcrumbUploadWorker>(15, TimeUnit.MINUTES)
                .setConstraints(
                    Constraints.Builder()
                        .setRequiredNetworkType(NetworkType.CONNECTED)
                        .build()
                )
                .build()
            WorkManager.getInstance(context).enqueueUniquePeriodicWork(
                WORK_NAME, ExistingPeriodicWorkPolicy.KEEP, request
            )
        }
    }
}
