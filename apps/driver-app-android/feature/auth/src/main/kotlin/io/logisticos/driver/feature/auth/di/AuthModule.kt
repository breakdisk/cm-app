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
     * Dev OTP bypass (123456) is intentionally restricted to the `dev` flavor.
     * stagingDebug and prodDebug must use real OTPs so drivers test against the
     * actual backend — fake sessions block all task/delivery API calls.
     * AppModule overrides this binding with the flavor-aware value.
     */
    @Provides
    @Named("dev_bypass_enabled")
    fun provideDevBypassEnabled(): Boolean = false
}
