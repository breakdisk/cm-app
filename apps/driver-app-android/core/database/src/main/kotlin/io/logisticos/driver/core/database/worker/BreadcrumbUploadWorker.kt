// Removed — GPS breadcrumb upload is now handled inside OutboundSyncWorker (step 4c
// of the POD_SUBMIT flow) which flushes offline breadcrumbs via DriverOpsApiService.bulkUpdateLocation().
// This class is kept as a no-op placeholder so WorkManager's HiltWorkerFactory doesn't fail
// if an old periodic work request exists in the WorkManager DB from a previous install.
// The schedule() method is never called from application code.
package io.logisticos.driver.core.database.worker

import android.content.Context
import androidx.hilt.work.HiltWorker
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import dagger.assisted.Assisted
import dagger.assisted.AssistedInject
import java.util.concurrent.TimeUnit

@HiltWorker
class BreadcrumbUploadWorker @AssistedInject constructor(
    @Assisted context: Context,
    @Assisted workerParams: WorkerParameters,
) : CoroutineWorker(context, workerParams) {

    override suspend fun doWork(): Result = Result.success()

    companion object {
        const val WORK_NAME = "breadcrumb_upload"

        fun schedule(context: Context) {
            // No-op: OutboundSyncWorker handles breadcrumb flushing.
        }
    }
}
