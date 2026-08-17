package net.cargomarket.omnideliv.courier

import android.app.Application
import android.app.NotificationChannel
import android.app.NotificationManager
import android.os.Build
import dagger.hilt.android.HiltAndroidApp

@HiltAndroidApp
class CourierApp : Application() {

    override fun onCreate() {
        super.onCreate()
        createShiftChannel()
    }

    /**
     * The channel the foreground location service posts into.
     *
     * Created at application start rather than when the service starts: a
     * foreground service whose channel does not exist yet is killed by the
     * system within seconds, and the symptom is a shift that silently stops
     * reporting rather than an error anyone sees.
     */
    private fun createShiftChannel() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        val channel = NotificationChannel(
            SHIFT_CHANNEL_ID,
            getString(R.string.shift_channel_name),
            // Low: a persistent notification a courier cannot dismiss should not
            // also make a sound every time the service restarts.
            NotificationManager.IMPORTANCE_LOW,
        )
        getSystemService(NotificationManager::class.java).createNotificationChannel(channel)
    }

    companion object {
        const val SHIFT_CHANNEL_ID = "shift"
    }
}
