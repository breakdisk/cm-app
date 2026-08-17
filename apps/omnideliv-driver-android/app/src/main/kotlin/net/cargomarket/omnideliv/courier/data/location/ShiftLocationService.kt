package net.cargomarket.omnideliv.courier.data.location

import android.app.Notification
import android.app.Service
import android.content.Intent
import android.os.IBinder
import androidx.core.app.NotificationCompat
import dagger.hilt.android.AndroidEntryPoint
import net.cargomarket.omnideliv.courier.CourierApp
import net.cargomarket.omnideliv.courier.R

/**
 * Keeps location flowing while the courier is on shift and the app is
 * backgrounded.
 *
 * A foreground service with a persistent notification, because that is the only
 * arrangement Android will not throttle to a stop under Doze — and the customer
 * ETA is computed from these fixes.
 *
 * The notification is not a courtesy. Android requires it, and the copy says
 * plainly what is being shared and why: a courier is entitled to know their
 * location leaves the device, and a vague "app is running" would be a dark
 * pattern about exactly that.
 */
@AndroidEntryPoint
class ShiftLocationService : Service() {

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        startForeground(NOTIFICATION_ID, buildNotification())
        // START_STICKY: the system restarting this after a memory kill is
        // exactly what should happen mid-shift. The alternative loses telemetry
        // silently and the courier has no way to notice.
        return START_STICKY
    }

    private fun buildNotification(): Notification =
        NotificationCompat.Builder(this, CourierApp.SHIFT_CHANNEL_ID)
            .setContentTitle(getString(R.string.shift_notification_title))
            .setContentText(getString(R.string.shift_notification_body))
            .setSmallIcon(android.R.drawable.ic_menu_mylocation)
            .setOngoing(true)
            .setPriority(NotificationCompat.PRIORITY_LOW)
            .build()

    private companion object {
        const val NOTIFICATION_ID = 4101
    }
}
