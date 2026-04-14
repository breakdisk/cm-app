package io.logisticos.driver.core.network.di

import com.jakewharton.retrofit2.converter.kotlinx.serialization.asConverterFactory
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.logisticos.driver.core.network.authenticator.TokenAuthenticator
import io.logisticos.driver.core.network.interceptor.AuthInterceptor
import io.logisticos.driver.core.network.interceptor.TenantInterceptor
import io.logisticos.driver.core.network.service.DirectionsApiService
import io.logisticos.driver.core.network.service.DriverOpsApiService
import io.logisticos.driver.core.network.service.IdentityApiService
import io.logisticos.driver.core.network.service.PodApiService
import io.logisticos.driver.core.network.service.TrackingApiService
import kotlinx.serialization.json.Json
import okhttp3.CertificatePinner
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.logging.HttpLoggingInterceptor
import retrofit2.Retrofit
import javax.inject.Named
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
object NetworkModule {

    // Pins the Let's Encrypt E7 intermediate (valid until Mar 2027) and ISRG Root X1
    // as fallback. Both rotate much less frequently than the leaf cert.
    // Update E7 pin when it approaches expiry: run
    //   echo | openssl s_client -connect os-api.cargomarket.net:443 -showcerts 2>/dev/null |
    //   openssl x509 -pubkey -noout | openssl pkey -pubin -outform der |
    //   openssl dgst -sha256 -binary | openssl enc -base64
    // on the second certificate in the chain.
    private val CERT_PINNER = CertificatePinner.Builder()
        .add("*.cargomarket.net",      "sha256/y7xVm0TVJNahMr2sZydE2jQH8SquXV9yLF9seROHHHU=") // LE E7 intermediate
        .add("*.cargomarket.net",      "sha256/YLh1dUR9y6Kja30RrAn7JKnbQG/uEtLMkBgFF2Fuihg=") // ISRG Root X1
        .build()

    @Provides @Singleton
    fun provideJson(): Json = Json {
        ignoreUnknownKeys = true
        isLenient = true
        encodeDefaults = true
    }

    @Provides @Singleton
    fun provideLoggingInterceptor(
        @Named("log_level") level: HttpLoggingInterceptor.Level
    ): HttpLoggingInterceptor = HttpLoggingInterceptor().apply { this.level = level }

    @Provides @Singleton
    fun provideOkHttpClient(
        authInterceptor: AuthInterceptor,
        tenantInterceptor: TenantInterceptor,
        tokenAuthenticator: TokenAuthenticator,
        loggingInterceptor: HttpLoggingInterceptor,
        @Named("is_debug") isDebug: Boolean
    ): OkHttpClient = OkHttpClient.Builder()
        .addInterceptor(authInterceptor)
        .addInterceptor(tenantInterceptor)
        .addInterceptor(loggingInterceptor)
        .authenticator(tokenAuthenticator)
        .apply { if (!isDebug) certificatePinner(CERT_PINNER) }
        .build()

    @Provides @Singleton
    fun provideRetrofit(
        okHttpClient: OkHttpClient,
        json: Json,
        @Named("base_url") baseUrl: String
    ): Retrofit = Retrofit.Builder()
        .baseUrl(baseUrl)
        .client(okHttpClient)
        .addConverterFactory(json.asConverterFactory("application/json".toMediaType()))
        .build()

    @Provides @Singleton
    fun provideIdentityApiService(retrofit: Retrofit): IdentityApiService =
        retrofit.create(IdentityApiService::class.java)

    @Provides @Singleton
    fun provideDriverOpsApiService(retrofit: Retrofit): DriverOpsApiService =
        retrofit.create(DriverOpsApiService::class.java)

    @Provides @Singleton
    fun provideDirectionsApiService(retrofit: Retrofit): DirectionsApiService =
        retrofit.create(DirectionsApiService::class.java)

    @Provides @Singleton
    fun providePodApiService(retrofit: Retrofit): PodApiService =
        retrofit.create(PodApiService::class.java)

    @Provides @Singleton
    fun provideTrackingApiService(retrofit: Retrofit): TrackingApiService =
        retrofit.create(TrackingApiService::class.java)
}
