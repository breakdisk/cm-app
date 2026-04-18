package io.logisticos.driver.core.location

import android.annotation.SuppressLint
import android.content.Context
import android.content.Intent
import android.os.CancellationSignal
import com.google.android.gms.location.CurrentLocationRequest
import com.google.android.gms.location.LocationServices
import com.google.android.gms.location.Priority
import dagger.hilt.android.qualifiers.ApplicationContext
import io.logisticos.driver.core.database.dao.LocationBreadcrumbDao
import kotlinx.coroutines.tasks.await
import kotlinx.coroutines.withTimeoutOrNull
import javax.inject.Inject
import javax.inject.Singleton

data class LatLng(val lat: Double, val lng: Double)

@Singleton
class LocationRepository @Inject constructor(
    @ApplicationContext private val context: Context,
    private val breadcrumbDao: LocationBreadcrumbDao
) {
    private val fusedClient by lazy { LocationServices.getFusedLocationProviderClient(context) }

    fun startShiftTracking(shiftId: String) {
        val intent = Intent(context, LocationForegroundService::class.java).apply {
            putExtra(LocationForegroundService.EXTRA_SHIFT_ID, shiftId)
        }
        context.startForegroundService(intent)
    }

    fun stopShiftTracking() {
        context.stopService(Intent(context, LocationForegroundService::class.java))
    }

    /**
     * Returns the last known location from FusedLocationProviderClient.
     * Returns null if permission not granted or location unavailable.
     * Caller must ensure ACCESS_FINE_LOCATION permission is held.
     */
    @SuppressLint("MissingPermission")
    suspend fun getLastKnownLocation(): LatLng? {
        return try {
            val loc = fusedClient.lastLocation.await()
            if (loc != null) LatLng(loc.latitude, loc.longitude) else null
        } catch (_: Exception) {
            null
        }
    }

    /**
     * Requests a fresh GPS fix (up to 5 s), falling back to last known.
     * Use when an accurate position is needed immediately (e.g. go-online).
     * Caller must ensure ACCESS_FINE_LOCATION permission is held.
     */
    @SuppressLint("MissingPermission")
    suspend fun getCurrentOrLastKnownLocation(): LatLng? {
        val fresh = withTimeoutOrNull(5_000L) {
            try {
                val req = CurrentLocationRequest.Builder()
                    .setPriority(Priority.PRIORITY_HIGH_ACCURACY)
                    .build()
                val loc = fusedClient.getCurrentLocation(req, CancellationSignal()).await()
                if (loc != null) LatLng(loc.latitude, loc.longitude) else null
            } catch (_: Exception) {
                null
            }
        }
        return fresh ?: getLastKnownLocation()
    }

    suspend fun getUnsyncedBreadcrumbs() = breadcrumbDao.getUnsynced()

    suspend fun markBreadcrumbsSynced(ids: List<Long>) = breadcrumbDao.markSynced(ids)

    suspend fun pruneOldBreadcrumbs(olderThanMs: Long) = breadcrumbDao.pruneOld(olderThanMs)
}
