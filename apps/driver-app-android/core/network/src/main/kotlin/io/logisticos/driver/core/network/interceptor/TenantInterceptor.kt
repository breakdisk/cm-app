package io.logisticos.driver.core.network.interceptor

import io.logisticos.driver.core.network.auth.SessionManager
import okhttp3.Interceptor
import okhttp3.Response
import javax.inject.Inject

class TenantInterceptor @Inject constructor(
    private val sessionManager: SessionManager
) : Interceptor {
    override fun intercept(chain: Interceptor.Chain): Response {
        val tenantId = sessionManager.getTenantId()
        val request = if (tenantId != null) {
            chain.request().newBuilder()
                .addHeader("X-Tenant-ID", tenantId)
                .build()
        } else {
            chain.request()
        }
        return chain.proceed(request)
    }
}
