package net.cargomarket.omnideliv.courier.data.di

import android.content.Context
import com.jakewharton.retrofit2.converter.kotlinx.serialization.asConverterFactory
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.android.qualifiers.ApplicationContext
import dagger.hilt.components.SingletonComponent
import kotlinx.serialization.json.Json
import net.cargomarket.omnideliv.courier.BuildConfig
import net.cargomarket.omnideliv.courier.data.CourierApi
import net.cargomarket.omnideliv.courier.data.TokenStore
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import retrofit2.Retrofit
import java.util.concurrent.TimeUnit
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
object NetworkModule {

    @Provides
    @Singleton
    fun tokenStore(@ApplicationContext context: Context): TokenStore = TokenStore(context)

    @Provides
    @Singleton
    fun json(): Json = Json {
        // The server adds fields over time — `offer_card`, `courier_user_id` —
        // and an app that refused an unknown one would break on every backend
        // deploy rather than ignoring what it does not yet render.
        ignoreUnknownKeys = true
        explicitNulls = false
    }

    @Provides
    @Singleton
    fun okHttp(tokens: TokenStore): OkHttpClient = OkHttpClient.Builder()
        // Short by web standards, deliberately. A courier on a dying cell
        // connection is better served by failing fast into the outbound queue,
        // which will retry, than by a request that hangs while they stand at a
        // door waiting for a spinner.
        .connectTimeout(10, TimeUnit.SECONDS)
        .readTimeout(20, TimeUnit.SECONDS)
        .addInterceptor { chain ->
            val token = tokens.accessToken
            val request = if (token.isNullOrBlank()) {
                chain.request()
            } else {
                chain.request().newBuilder()
                    .header("Authorization", "Bearer $token")
                    .build()
            }
            chain.proceed(request)
        }
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
}
