package io.logisticos.driver.core.location

import android.content.Context
import android.content.Intent
import dagger.hilt.android.qualifiers.ApplicationContext
import io.logisticos.driver.core.database.dao.LocationBreadcrumbDao
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class LocationRepository @Inject constructor(
    @ApplicationContext private val context: Context,
    private val breadcrumbDao: LocationBreadcrumbDao
) {
    fun startShiftTracking(shiftId: String) {
        val intent = Intent(context, LocationForegroundService::class.java).apply {
            putExtra(LocationForegroundService.EXTRA_SHIFT_ID, shiftId)
        }
        context.startForegroundService(intent)
    }

    fun stopShiftTracking() {
        context.stopService(Intent(context, LocationForegroundService::class.java))
    }

    suspend fun getUnsyncedBreadcrumbs() = breadcrumbDao.getUnsynced()

    suspend fun markBreadcrumbsSynced(ids: List<Long>) = breadcrumbDao.markSynced(ids)

    suspend fun pruneOldBreadcrumbs(olderThanMs: Long) = breadcrumbDao.pruneOld(olderThanMs)
}
