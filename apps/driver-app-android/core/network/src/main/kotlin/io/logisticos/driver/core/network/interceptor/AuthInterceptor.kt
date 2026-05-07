package io.logisticos.driver.core.network.interceptor

import io.logisticos.driver.core.network.auth.SessionManager
import okhttp3.Interceptor
import okhttp3.Response
import javax.inject.Inject

class AuthInterceptor @Inject constructor(
    private val sessionManager: SessionManager
) : Interceptor {
    override fun intercept(chain: Interceptor.Chain): Response {
        val request = chain.request()

        // Don't leak our JWT to third-party storage endpoints. The OkHttpClient
        // is shared with R2 photo uploads (presigned PUTs) — if we send
        // `Authorization: Bearer <JWT>` to R2, it tries to parse the JWT as an
        // AWS4-HMAC-SHA256 signature, fails, and rejects the request with a
        // misleading "No date provided in x-amz-date" error. Same hazard for
        // S3, GCS, Azure Blob, and any other presigned-URL upload target.
        if (isExternalStorageHost(request.url.host)) {
            return chain.proceed(request)
        }

        val jwt = sessionManager.getJwt()
        val authed = if (jwt != null) {
            request.newBuilder()
                .addHeader("Authorization", "Bearer $jwt")
                .build()
        } else {
            request
        }
        return chain.proceed(authed)
    }

    private fun isExternalStorageHost(host: String): Boolean =
        host.endsWith("r2.cloudflarestorage.com") ||
                host.endsWith("amazonaws.com") ||
                host.endsWith("googleapis.com") ||
                host.endsWith("blob.core.windows.net")
}
