package io.logisticos.driver.feature.scanner.di

import android.content.Context
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.android.components.ActivityRetainedComponent
import dagger.hilt.android.qualifiers.ApplicationContext
import io.logisticos.driver.feature.scanner.data.HardwareScannerManager
import io.logisticos.driver.feature.scanner.data.MlKitScannerManager
import io.logisticos.driver.feature.scanner.domain.ScannerManager

@Module
@InstallIn(ActivityRetainedComponent::class)
object ScannerModule {
    @Provides
    fun provideScannerManager(
        @ApplicationContext context: Context,
        mlKit: MlKitScannerManager,
        hardware: HardwareScannerManager
    ): ScannerManager {
        val pm = context.packageManager
        fun isInstalled(packageName: String): Boolean = runCatching {
            pm.getPackageInfo(packageName, 0)
        }.isSuccess
        val isZebra = isInstalled("com.symbol.datawedge")
        val isHoneywell = isInstalled("com.honeywell.aidc")
        return if (isZebra || isHoneywell) hardware else mlKit
    }
}
