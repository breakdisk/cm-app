package io.logisticos.driver.core.network.interceptor

import io.logisticos.driver.core.network.auth.SessionManager
import okhttp3.Interceptor
import okhttp3.Response
import javax.inject.Inject

class AuthInterceptor @Inject constructor(
    private val sessionManager: SessionManager
) : Interceptor {
    override fun intercept(chain: Interceptor.Chain): Response {
        val jwt = sessionManager.getJwt()
        val request = if (jwt != null) {
            chain.request().newBuilder()
                .addHeader("Authorization", "Bearer $jwt")
                .build()
        } else {
            chain.request()
        }
        return chain.proceed(request)
    }
}
