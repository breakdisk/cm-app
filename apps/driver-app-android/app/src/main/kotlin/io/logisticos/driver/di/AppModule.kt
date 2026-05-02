package io.logisticos.driver.di

import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.logisticos.driver.BuildConfig
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import okhttp3.logging.HttpLoggingInterceptor
import javax.inject.Named
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
object AppModule {

    @Provides
    @Singleton
    @Named("base_url")
    fun provideBaseUrl(): String = BuildConfig.BASE_URL

    @Provides
    @Named("log_level")
    fun provideLogLevel(): HttpLoggingInterceptor.Level =
        if (BuildConfig.DEBUG) HttpLoggingInterceptor.Level.BODY
        else HttpLoggingInterceptor.Level.NONE

    @Provides
    @Named("is_debug")
    fun provideIsDebug(): Boolean = BuildConfig.DEBUG

    @Provides
    @Named("maps_api_key")
    fun provideMapsApiKey(): String = BuildConfig.MAPS_API_KEY

    @Provides
    @Singleton
    @Named("tenant_slug")
    fun provideTenantSlug(): String = BuildConfig.TENANT_ID

    @Provides
    @Singleton
    @Named("application_scope")
    fun provideApplicationScope(): CoroutineScope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    /** OTP bypass (123456) only active in devDebug — stagingDebug/prodDebug hit the real backend. */
    @Provides
    @Named("dev_bypass_enabled")
    fun provideDevBypassEnabled(): Boolean = BuildConfig.DEBUG && BuildConfig.FLAVOR == "dev"
}
