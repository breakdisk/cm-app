package net.cargomarket.omnideliv.courier.data.di

import com.jakewharton.retrofit2.converter.kotlinx.serialization.asConverterFactory
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import kotlinx.serialization.json.Json
import net.cargomarket.omnideliv.courier.BuildConfig
import net.cargomarket.omnideliv.courier.data.ComplianceApi
import net.cargomarket.omnideliv.courier.data.CourierApi
import net.cargomarket.omnideliv.courier.data.RefreshApi
import net.cargomarket.omnideliv.courier.data.RefreshAuthenticator
import net.cargomarket.omnideliv.courier.data.TokenStore
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import retrofit2.Retrofit
import java.util.concurrent.TimeUnit
import javax.inject.Provider
import javax.inject.Qualifier
import javax.inject.Singleton

/** Marks the bare client used for refreshing, which must not carry the authenticator. */
@Qualifier
@Retention(AnnotationRetention.BINARY)
annotation class RefreshClient

@Module
@InstallIn(SingletonComponent::class)
object NetworkModule {

    // No @Provides for TokenStore. It carries its own `@Inject constructor`, and
    // declaring both is a duplicate binding Hilt rejects at build time.

    @Provides
    @Singleton
    // One definition, shared with the wire-contract tests — see CourierJson.
    fun json(): Json = net.cargomarket.omnideliv.courier.data.CourierJson

    /**
     * A client with **no authenticator**, used only to refresh the session.
     *
     * Refreshing through the main client would send the refresh call back
     * through the authenticator on its own 401, forever.
     */
    @Provides
    @Singleton
    @RefreshClient
    fun refreshOkHttp(): OkHttpClient = OkHttpClient.Builder()
        .connectTimeout(10, TimeUnit.SECONDS)
        .readTimeout(20, TimeUnit.SECONDS)
        .build()

    @Provides
    @Singleton
    fun refreshApi(@RefreshClient client: OkHttpClient, json: Json): RefreshApi =
        Retrofit.Builder()
            .baseUrl(BuildConfig.API_BASE_URL)
            .client(client)
            .addConverterFactory(json.asConverterFactory("application/json".toMediaType()))
            .build()
            .create(RefreshApi::class.java)

    @Provides
    @Singleton
    fun okHttp(tokens: TokenStore, refreshApi: Provider<RefreshApi>): OkHttpClient = OkHttpClient.Builder()
        // Short by web standards, deliberately. A courier on a dying cell
        // connection is better served by failing fast into the outbound queue,
        // which will retry, than by a request that hangs while they stand at a
        // door waiting for a spinner.
        .connectTimeout(10, TimeUnit.SECONDS)
        .readTimeout(20, TimeUnit.SECONDS)
        .addInterceptor { chain ->
            val token = tokens.accessToken
            // Never on `v1/auth/*`. Those endpoints establish a session rather
            // than consume one, and attaching a dying token to the OTP verify
            // that is *replacing* it makes a 401 there look like an expired
            // session — which would spend the refresh token on a request that
            // never needed one.
            val isAuth = chain.request().url.encodedPath.contains("/v1/auth/")
            val request = if (token.isNullOrBlank() || isAuth) {
                chain.request()
            } else {
                chain.request().newBuilder()
                    .header("Authorization", "Bearer $token")
                    .build()
            }
            chain.proceed(request)
        }
        // Only fires on a 401, and only on requests that carried a token. The
        // access token lives an hour; a shift does not.
        .authenticator(RefreshAuthenticator(tokens) { refreshApi.get() })
        .build()

    @Provides
    @Singleton
    fun retrofit(client: OkHttpClient, json: Json): Retrofit = Retrofit.Builder()
        .baseUrl(BuildConfig.API_BASE_URL)
        .client(client)
        .addConverterFactory(json.asConverterFactory("application/json".toMediaType()))
        .build()

    @Provides
    @Singleton
    fun courierApi(retrofit: Retrofit): CourierApi = retrofit.create(CourierApi::class.java)

    /**
     * Same Retrofit, same client, same bearer.
     *
     * A separate interface rather than more methods on [CourierApi] because
     * compliance is a different service with a different path prefix
     * (`api/v1/`) and a response envelope the other two do not use — folding it
     * in would put three contracts behind one type.
     */
    @Provides
    @Singleton
    fun complianceApi(retrofit: Retrofit): ComplianceApi =
        retrofit.create(ComplianceApi::class.java)
}
