package io.logisticos.driver.core.network.interceptor

import io.logisticos.driver.core.network.auth.SessionManager
import okhttp3.Interceptor
import okhttp3.Response
import javax.inject.Inject

class TenantInterceptor @Inject constructor(
    private val sessionManager: SessionManager
) : Interceptor {
    override fun intercept(chain: Interceptor.Chain): Response {
        val request = chain.request()

        // Same reasoning as AuthInterceptor: never inject our tenant header on
        // third-party storage uploads. Cloudflare R2 in particular rejects the
        // PUT with a confusing "No date provided in x-amz-date" error if any
        // unexpected non-AWS header is present alongside a presigned URL.
        if (isExternalStorageHost(request.url.host)) {
            return chain.proceed(request)
        }

        val tenantId = sessionManager.getTenantId()
        val tenanted = if (tenantId != null) {
            request.newBuilder()
                .addHeader("X-Tenant-ID", tenantId)
                .build()
        } else {
            request
        }
        return chain.proceed(tenanted)
    }

    private fun isExternalStorageHost(host: String): Boolean =
        host.endsWith("r2.cloudflarestorage.com") ||
                host.endsWith("amazonaws.com") ||
                host.endsWith("googleapis.com") ||
                host.endsWith("blob.core.windows.net")
}
