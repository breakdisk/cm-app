package net.cargomarket.omnideliv.courier.data.location

import android.app.Notification
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationCompat
import com.google.android.gms.location.LocationCallback
import com.google.android.gms.location.LocationRequest
import com.google.android.gms.location.LocationResult
import com.google.android.gms.location.LocationServices
import com.google.android.gms.location.Priority
import dagger.hilt.android.AndroidEntryPoint
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import net.cargomarket.omnideliv.courier.R
import net.cargomarket.omnideliv.courier.data.CourierApi
import net.cargomarket.omnideliv.courier.data.PositionRequest
import net.cargomarket.omnideliv.courier.data.TokenStore
import javax.inject.Inject

/**
 * Streams the courier's position while they are on duty.
 *
 * A foreground service with a persistent notification because Android requires
 * one, and because a courier is entitled to see plainly that their location is
 * being reported — this runs only between going on duty and going off it.
 *
 * Batched HTTP rather than a socket. field-ops is stateless and the platform
 * requires rolling updates, so a socket server would drop every courier on every
 * deploy; and a backgrounded socket dies to Doze anyway, which means building the
 * HTTP path regardless and then maintaining two.
 */
@AndroidEntryPoint
class ShiftLocationService : Service() {

    @Inject lateinit var api: CourierApi
    @Inject lateinit var tokens: TokenStore

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val client by lazy { LocationServices.getFusedLocationProviderClient(this) }

    private val callback = object : LocationCallback() {
        override fun onLocationResult(result: LocationResult) {
            val fix = result.lastLocation ?: return
            val courierId = tokens.courierId ?: return
            scope.launch {
                // Fire and forget. A dropped fix is not worth retrying: the next
                // one is seconds away and is strictly better information. The
                // outbound queue exists for milestones, which are not
                // replaceable; a position is.
                runCatching {
                    api.position(
                        courierId,
                        PositionRequest(
                            lat = fix.latitude,
                            lng = fix.longitude,
                            deviceTimestamp = isoNow(fix.time),
                        ),
                    )
                }
            }
        }
    }

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        // The type argument is mandatory from Android 14. This service declares
        // `foregroundServiceType="location"` in the manifest, and on API 34+ the
        // two-argument call throws MissingForegroundServiceTypeException — which
        // would crash the app the moment a courier goes on shift. Below 34 the
        // three-argument overload does not exist, hence the branch.
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            startForeground(
                NOTIFICATION_ID,
                buildNotification(),
                ServiceInfo.FOREGROUND_SERVICE_TYPE_LOCATION,
            )
        } else {
            startForeground(NOTIFICATION_ID, buildNotification())
        }

        requestUpdates()

        // START_STICKY: the system restarting this after a memory kill is
        // exactly what should happen mid-shift. The alternative loses telemetry
        // silently and the courier has no way to notice.
        return START_STICKY
    }

    private fun requestUpdates() {
        val request = LocationRequest.Builder(Priority.PRIORITY_HIGH_ACCURACY, SAMPLE_MS)
            // The floor stops a stationary courier burning battery on fixes that
            // say the same thing, without letting a moving one go unreported.
            .setMinUpdateIntervalMillis(MIN_SAMPLE_MS)
            .build()

        // Permission is checked by the caller before the service is started;
        // this still catches the revoke-while-running case, where the throw is
        // the only signal.
        runCatching { client.requestLocationUpdates(request, callback, mainLooper) }
    }

    override fun onDestroy() {
        client.removeLocationUpdates(callback)
        scope.cancel()
        super.onDestroy()
    }

    private fun buildNotification(): Notification =
        NotificationCompat.Builder(this, CHANNEL_ID)
            .setContentTitle(getString(R.string.shift_notification_title))
            .setContentText(getString(R.string.shift_notification_text))
            .setSmallIcon(android.R.drawable.ic_menu_mylocation)
            .setOngoing(true)
            .build()

    companion object {
        const val CHANNEL_ID = "shift"
        private const val NOTIFICATION_ID = 1

        /** Ten seconds while a job is live, per the spec's cadence. */
        private const val SAMPLE_MS = 10_000L
        private const val MIN_SAMPLE_MS = 5_000L

        fun start(context: Context) {
            val intent = Intent(context, ShiftLocationService::class.java)
            // startForegroundService is required from Oreo; minSdk is 26, so it
            // is always the right call here.
            context.startForegroundService(intent)
        }

        fun stop(context: Context) {
            context.stopService(Intent(context, ShiftLocationService::class.java))
        }

        private fun isoNow(millis: Long): String =
            java.time.format.DateTimeFormatter.ISO_OFFSET_DATE_TIME.format(
                java.time.Instant.ofEpochMilli(millis).atOffset(java.time.ZoneOffset.UTC),
            )
    }
}
