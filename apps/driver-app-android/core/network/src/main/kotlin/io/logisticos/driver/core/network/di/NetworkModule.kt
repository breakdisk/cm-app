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
import io.logisticos.driver.core.network.service.TrackingApiService
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
                // Pins three anchors to survive cert rotation across Cloudflare edges:
                //   • Leaf cert SubjectPublicKeyInfo (CN=cargomarket.net, ECDSA P-256)
                //   • Let's Encrypt E7 intermediate (valid until Mar 2027)
                //   • ISRG Root X1 ultimate root
                //
                // OkHttp passes if ANY pin matches a cert in the verified chain. Three
                // anchors mean rotating any single one (e.g. when LE auto-renews the
                // leaf or E7 expires) keeps the app working.
                //
                // Recompute pins before each rotation:
                //   echo | openssl s_client -connect os-api.cargomarket.net:443 -showcerts 2>/dev/null |
                //   openssl x509 -pubkey -noout | openssl pkey -pubin -outform der |
                //   openssl dgst -sha256 -binary | openssl enc -base64
                certificatePinner(
                    CertificatePinner.Builder()
                        // Leaf — rotates every ~90 days; verify after every renewal
                        .add("*.cargomarket.net", "sha256/Xmbu6WAH7f8fcGMz/e4qRPr9oWEOgvmm9x/yz6couhU=")
                        // LE E7 intermediate
                        .add("*.cargomarket.net", "sha256/y7xVm0TVJNahMr2sZydE2jQH8SquXV9yLF9seROHHHU=")
                        // ISRG Root X1 — long-lived backstop
                        .add("*.cargomarket.net", "sha256/YLh1dUR9y6Kja30RrAn7JKnbQG/uEtLMkBgFF2Fuihg=")
                        .build()
                )
            }
        }
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

    @Provides @Singleton
    fun provideComplianceApiService(retrofit: Retrofit): ComplianceApiService =
        retrofit.create(ComplianceApiService::class.java)
}
