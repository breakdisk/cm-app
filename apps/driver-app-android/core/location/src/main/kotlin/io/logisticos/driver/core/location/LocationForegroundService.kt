package io.logisticos.driver.core.location

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Intent
import android.os.IBinder
import androidx.core.app.NotificationCompat
import com.google.android.gms.location.FusedLocationProviderClient
import com.google.android.gms.location.LocationCallback
import com.google.android.gms.location.LocationRequest
import com.google.android.gms.location.LocationResult
import com.google.android.gms.location.LocationServices
import com.google.android.gms.location.Priority
import dagger.hilt.android.AndroidEntryPoint
import io.logisticos.driver.core.database.dao.LocationBreadcrumbDao
import io.logisticos.driver.core.database.entity.LocationBreadcrumbEntity
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import javax.inject.Inject

@AndroidEntryPoint
class LocationForegroundService : Service() {

    @Inject lateinit var breadcrumbDao: LocationBreadcrumbDao
    @Inject lateinit var locationRepository: LocationRepository

    private lateinit var fusedClient: FusedLocationProviderClient
    private lateinit var locationCallback: LocationCallback
    private val scope = CoroutineScope(Dispatchers.IO + SupervisorJob())

    private var currentShiftId: String = ""
    private var lastMovementTime = System.currentTimeMillis()
    private var isStationary = false
    // Guard: startLocationUpdates() must only run once per service lifecycle.
    // startForegroundService() on an already-running service re-delivers
    // onStartCommand() without creating a new instance — without this flag
    // each call would register an additional LocationCallback, flooding the
    // FLP with duplicate requests (visible as repeated RequestManager_FLP
    // entries in logcat).
    private var updatesStarted = false

    override fun onCreate() {
        super.onCreate()
        fusedClient = LocationServices.getFusedLocationProviderClient(this)
        createNotificationChannel()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        currentShiftId = intent?.getStringExtra(EXTRA_SHIFT_ID) ?: ""
        // Empty shiftId = availability-tracking mode: GPS runs and publishes to
        // the SharedFlow (keeping dispatch's driver_locations populated) but no
        // breadcrumbs are recorded (they require a real shift context).
        val label = if (currentShiftId.isEmpty()) "Available for dispatch" else "Shift active"
        startForeground(NOTIFICATION_ID, buildNotification(label))
        if (!updatesStarted) {
            startLocationUpdates()
            updatesStarted = true
        }
        return START_STICKY
    }

    private fun startLocationUpdates() {
        locationCallback = object : LocationCallback() {
            override fun onLocationResult(result: LocationResult) {
                result.lastLocation?.let { location ->
                    val speed = location.speed
                    val now = System.currentTimeMillis()

                    if (speed > 0.5f) lastMovementTime = now
                    isStationary = (now - lastMovementTime) > AdaptiveLocationManager.STATIONARY_THRESHOLD_MS

                    val newInterval = if (isStationary)
                        AdaptiveLocationManager.INTERVAL_STATIONARY_MS
                    else
                        AdaptiveLocationManager.intervalForSpeed(speed)

                    val shiftId = currentShiftId
                    val lat = location.latitude
                    val lng = location.longitude
                    scope.launch {
                        // Only record breadcrumbs when tracking a real shift.
                        // Empty shiftId = availability mode (driver online, no
                        // active shift yet) — still need to push location so
                        // dispatch's proximity query can find the driver.
                        if (shiftId.isNotEmpty()) {
                            breadcrumbDao.insert(
                                LocationBreadcrumbEntity(
                                    shiftId = shiftId,
                                    lat = lat,
                                    lng = lng,
                                    accuracy = location.accuracy,
                                    speedMps = speed,
                                    bearing = location.bearing,
                                    timestamp = now
                                )
                            )
                        }
                        // Always publish — this is what keeps driver_locations
                        // populated whether or not a shift is in progress.
                        locationRepository.publishLocation(lat, lng)
                    }

                    fusedClient.removeLocationUpdates(locationCallback)
                    requestUpdates(newInterval)
                }
            }
        }
        requestUpdates(AdaptiveLocationManager.intervalForSpeed(0f))
    }

    private fun requestUpdates(intervalMs: Long) {
        val request = LocationRequest.Builder(Priority.PRIORITY_HIGH_ACCURACY, intervalMs)
            .setMinUpdateIntervalMillis(intervalMs / 2)
            .build()
        try {
            fusedClient.requestLocationUpdates(request, locationCallback, mainLooper)
        } catch (e: SecurityException) {
            stopSelf()
        }
    }

    override fun onDestroy() {
        if (::locationCallback.isInitialized) {
            fusedClient.removeLocationUpdates(locationCallback)
        }
        updatesStarted = false
        scope.cancel()
        super.onDestroy()
    }

    override fun onBind(intent: Intent?): IBinder? = null

    private fun createNotificationChannel() {
        val channel = NotificationChannel(
            CHANNEL_ID,
            "Shift Tracking",
            NotificationManager.IMPORTANCE_LOW
        )
        getSystemService(NotificationManager::class.java).createNotificationChannel(channel)
    }

    private fun buildNotification(text: String): Notification =
        NotificationCompat.Builder(this, CHANNEL_ID)
            .setContentTitle("LogisticOS — Shift Active")
            .setContentText(text)
            .setSmallIcon(android.R.drawable.ic_menu_mylocation)
            .setOngoing(true)
            .build()

    companion object {
        const val CHANNEL_ID = "location_service"
        const val NOTIFICATION_ID = 1001
        const val EXTRA_SHIFT_ID = "shift_id"
    }
}
