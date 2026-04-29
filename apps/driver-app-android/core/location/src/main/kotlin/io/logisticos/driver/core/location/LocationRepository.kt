package io.logisticos.driver.core.location

import android.annotation.SuppressLint
import android.content.Context
import android.content.Intent
import com.google.android.gms.location.CurrentLocationRequest
import com.google.android.gms.location.LocationServices
import com.google.android.gms.location.Priority
import com.google.android.gms.tasks.CancellationTokenSource
import dagger.hilt.android.qualifiers.ApplicationContext
import io.logisticos.driver.core.database.dao.LocationBreadcrumbDao
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.asSharedFlow
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

    // ── Location broadcast ────────────────────────────────────────────────────
    //
    // LocationForegroundService publishes every GPS fix it receives here via
    // publishLocation(). HomeViewModel collects this Flow and forwards each fix
    // to the backend API (driver_ops.driver_locations). This keeps the network
    // layer out of the location module while still wiring service → backend.
    //
    // replay = 1  — new collectors (e.g. ViewModel recreated) get the last fix
    //              immediately so dispatch can see the driver without waiting
    //              for the next heartbeat.
    // DROP_OLDEST — if the collector is slow we never block the GPS callback.
    private val _locationUpdates = MutableSharedFlow<LatLng>(
        replay = 1,
        extraBufferCapacity = 8,
        onBufferOverflow = BufferOverflow.DROP_OLDEST
    )
    val locationUpdates: SharedFlow<LatLng> = _locationUpdates.asSharedFlow()

    /**
     * Called by [LocationForegroundService] on every GPS fix.
     * Non-suspending — safe to call from a LocationCallback.
     */
    fun publishLocation(lat: Double, lng: Double) {
        _locationUpdates.tryEmit(LatLng(lat, lng))
    }

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
        // PRIORITY_BALANCED_POWER_ACCURACY uses network/WiFi positioning which
        // works indoors and returns a fix in ~1-2 s. PRIORITY_HIGH_ACCURACY
        // requires a GPS satellite lock (can take 30+ s indoors or return null).
        // We extend the timeout to 8 s to cover weak indoor network signals.
        val cts = CancellationTokenSource()
        val fresh = withTimeoutOrNull(8_000L) {
            try {
                val req = CurrentLocationRequest.Builder()
                    .setPriority(Priority.PRIORITY_BALANCED_POWER_ACCURACY)
                    .build()
                val loc = fusedClient.getCurrentLocation(req, cts.token).await()
                if (loc != null) LatLng(loc.latitude, loc.longitude) else null
            } catch (_: Exception) {
                null
            }
        }
        cts.cancel()
        return fresh ?: getLastKnownLocation()
    }

    suspend fun getUnsyncedBreadcrumbs() = breadcrumbDao.getUnsynced()

    suspend fun markBreadcrumbsSynced(ids: List<Long>) = breadcrumbDao.markSynced(ids)

    suspend fun pruneOldBreadcrumbs(olderThanMs: Long) = breadcrumbDao.pruneOld(olderThanMs)
}
