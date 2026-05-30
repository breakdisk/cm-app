package io.logisticos.driver.core.network.di

import com.jakewharton.retrofit2.converter.kotlinx.serialization.asConverterFactory
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.logisticos.driver.core.network.authenticator.TokenAuthenticator
import io.logisticos.driver.core.network.interceptor.AuthInterceptor
import io.logisticos.driver.core.network.interceptor.TenantInterceptor
import io.logisticos.driver.core.network.service.ComplianceApiService
import io.logisticos.driver.core.network.service.DirectionsApiService
import io.logisticos.driver.core.network.service.DriverOpsApiService
import io.logisticos.driver.core.network.service.IdentityApiService
import io.logisticos.driver.core.network.service.PodApiService
import kotlinx.serialization.json.Json
import okhttp3.CertificatePinner
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.logging.HttpLoggingInterceptor
import retrofit2.Retrofit
import java.util.concurrent.TimeUnit
import javax.inject.Named
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
object NetworkModule {

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
        // Explicit timeouts — OkHttp 4 defaults are 10s connect/read/write with no
        // overall call timeout. We add a 30s call timeout so a stalled server
        // (e.g. identity pod restarting) fails fast instead of keeping isLoading=true.
        .connectTimeout(15, TimeUnit.SECONDS)
        .readTimeout(15, TimeUnit.SECONDS)
        .writeTimeout(15, TimeUnit.SECONDS)
        .callTimeout(30, TimeUnit.SECONDS)
        .addInterceptor(authInterceptor)
        .addInterceptor(tenantInterceptor)
        .addInterceptor(loggingInterceptor)
        .authenticator(tokenAuthenticator)
        .apply {
            if (!isDebug) {
                // Pin Let's Encrypt's long-lived ISRG ROOTS — never the leaf or
                // intermediate. The server uses LE certs: the leaf (CN=cargomarket.net)
                // rotates every ~90 days and the intermediate changes periodically
                // (currently ECDSA "E8"). The old pin-set below pinned the leaf + a
                // since-rotated intermediate, so every renewal bricked the app — that
                // is the "Certificate pinning failure!" seen in release-79.
                //
                // Both pins below are confirmed against the live chain
                // (os-api.cargomarket.net → ... → CN=ISRG Root X1). OkHttp passes if
                // ANY pin matches ANY cert in the verified chain, so pinning the roots
                // survives all leaf/intermediate rotation while still constraining the
                // trust anchor to Let's Encrypt.
                //
                // Verify the chain still terminates at an ISRG root after major CA changes:
                //   echo | openssl s_client -connect os-api.cargomarket.net:443 -showcerts 2>/dev/null |
                //   openssl x509 -pubkey -noout | openssl pkey -pubin -outform der |
                //   openssl dgst -sha256 -binary | openssl enc -base64
                certificatePinner(
                    CertificatePinner.Builder()
                        // ISRG Root X1 (RSA) — current live trust anchor
                        .add("*.cargomarket.net", "sha256/C5+lpZ7tcVwmwQIMcRtPbsQtWLABXhQzejna0wHFr8M=")
                        // ISRG Root X2 (ECDSA) — backup anchor for LE's ECDSA chain
                        .add("*.cargomarket.net", "sha256/diGVwiVYbubAI3RW4hB9xU8e/CH2GnkuvVFZE8zmgzI=")
                        .build()
                )
            }
        }
        .build()

    /**
     * Dedicated OkHttpClient for Cloudflare R2 presigned PUT uploads.
     *
     * Cannot share the API client because:
     *  1. Timeouts: writeTimeout(120s) covers a ~10 MB upload at 1 Mbps (3G floor).
     *     The API client's callTimeout(30s) kills uploads mid-stream on weak signal.
     *  2. TenantInterceptor adds X-Tenant-ID, which isn't in the presigned
     *     SignedHeaders — R2 would reject with SignatureDoesNotMatch.
     *  3. CertificatePinner pins *.cargomarket.net; R2 endpoints are
     *     *.r2.cloudflarestorage.com — pin mismatch would crash every upload.
     *
     * AuthInterceptor is already R2-aware (skips auth for storage hosts) but
     * is excluded here for defence-in-depth.
     */
    @Provides @Singleton @Named("r2_upload")
    fun provideR2UploadClient(
        loggingInterceptor: HttpLoggingInterceptor
    ): OkHttpClient = OkHttpClient.Builder()
        .connectTimeout(30, TimeUnit.SECONDS)
        .readTimeout(60, TimeUnit.SECONDS)
        .writeTimeout(120, TimeUnit.SECONDS)
        // No callTimeout — let writeTimeout bound each I/O chunk; a hard
        // wall-clock cap races with legitimate slow-upload scenarios.
        .addInterceptor(loggingInterceptor)
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
    fun provideComplianceApiService(retrofit: Retrofit): ComplianceApiService =
        retrofit.create(ComplianceApiService::class.java)
}
