package net.cargomarket.omnideliv.courier

import android.app.Application
import android.app.NotificationChannel
import android.app.NotificationManager
import android.os.Build
import androidx.hilt.work.HiltWorkerFactory
import androidx.work.Configuration
import dagger.hilt.android.HiltAndroidApp
import net.cargomarket.omnideliv.courier.data.sync.WorkManagerSyncScheduler
import javax.inject.Inject

@HiltAndroidApp
class CourierApp : Application(), Configuration.Provider {

    /**
     * Lets WorkManager construct workers that have constructor dependencies.
     * Without it the drain worker cannot be given the outbound repository and
     * every run fails at instantiation — silently, in the background, which is
     * the worst place for it.
     */
    @Inject lateinit var workerFactory: HiltWorkerFactory

    override val workManagerConfiguration: Configuration
        get() = Configuration.Builder()
            .setWorkerFactory(workerFactory)
            .build()

    override fun onCreate() {
        super.onCreate()
        createShiftChannel()
        // After super.onCreate, so injection has run and WorkManager can
        // initialise on demand against the configuration above.
        WorkManagerSyncScheduler.scheduleSafetyNet(this)
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
