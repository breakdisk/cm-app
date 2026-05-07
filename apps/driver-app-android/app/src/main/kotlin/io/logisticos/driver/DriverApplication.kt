package io.logisticos.driver

import android.app.Application
import androidx.hilt.work.HiltWorkerFactory
import androidx.work.Configuration
import com.mapbox.common.MapboxOptions
import dagger.hilt.android.HiltAndroidApp
import io.logisticos.driver.core.database.worker.NetworkConnectivityObserver
import javax.inject.Inject

@HiltAndroidApp
class DriverApplication : Application(), Configuration.Provider {

    @Inject lateinit var workerFactory: HiltWorkerFactory

    private lateinit var connectivityObserver: NetworkConnectivityObserver

    override val workManagerConfiguration: Configuration
        get() = Configuration.Builder()
            .setWorkerFactory(workerFactory)
            .build()

    override fun onCreate() {
        super.onCreate()
        if (BuildConfig.MAPBOX_ACCESS_TOKEN.isNotEmpty()) {
            MapboxOptions.accessToken = BuildConfig.MAPBOX_ACCESS_TOKEN
        }
        connectivityObserver = NetworkConnectivityObserver(this)
        connectivityObserver.register()
    }
}
