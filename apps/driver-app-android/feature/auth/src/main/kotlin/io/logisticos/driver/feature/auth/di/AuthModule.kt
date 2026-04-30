package io.logisticos.driver.feature.auth.di

import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.logisticos.driver.feature.auth.BuildConfig
import javax.inject.Named

@Module
@InstallIn(SingletonComponent::class)
object AuthModule {

    /**
     * Provides whether the dev OTP bypass is enabled.
     * `BuildConfig.DEBUG` is `true` for `debug` build type, `false` for `release`.
     * Library modules receive this correctly from the consuming app's build type.
     */
    @Provides
    @Named("dev_bypass_enabled")
    fun provideDevBypassEnabled(): Boolean = BuildConfig.DEBUG
}
