package io.logisticos.driver.feature.auth.data

import com.google.firebase.messaging.FirebaseMessaging
import io.logisticos.driver.core.network.auth.SessionManager
import io.logisticos.driver.core.network.service.IdentityApiService
import io.logisticos.driver.core.network.service.OtpSendRequest
import io.logisticos.driver.core.network.service.OtpVerifyRequest
import io.logisticos.driver.core.network.service.RegisterPushTokenRequest
import kotlinx.coroutines.tasks.await
import javax.inject.Inject
import javax.inject.Named
import javax.inject.Singleton

@Singleton
class AuthRepository @Inject constructor(
    private val apiService: IdentityApiService,
    private val sessionManager: SessionManager,
    @Named("tenant_slug") private val tenantSlug: String,
    /** True only in debug builds — gates the 123456 OTP shortcut for local development. */
    @Named("dev_bypass_enabled") private val devBypassEnabled: Boolean,
) {
    suspend fun sendOtp(phone: String): Result<Unit> = runCatching {
        apiService.sendOtp(OtpSendRequest(phone = phone, tenantSlug = tenantSlug, role = "driver"))
        Unit
    }

    suspend fun verifyOtp(phone: String, otp: String): Result<Unit> = runCatching {
        if (devBypassEnabled && otp == "123456") {
            // Dev shortcut — skip real OTP verification in debug builds only.
            sessionManager.saveTokens(jwt = "dev-token", refreshToken = "dev-refresh")
            sessionManager.saveTenantId("dev-tenant-id")
            sessionManager.saveDriverId("dev-driver-id")
            return@runCatching
        }
        val response = apiService.verifyOtp(
            OtpVerifyRequest(phone = phone, otp = otp, tenantSlug = tenantSlug, role = "driver")
        ).data
        sessionManager.saveTokens(jwt = response.jwt, refreshToken = response.refreshToken)
        sessionManager.saveTenantId(response.tenantId)
        sessionManager.saveDriverId(response.driverId)
        // Register FCM token now that we have a valid session.
        // onNewToken() fires only on first install / token rotation — it doesn't retry
        // after login, so we must do it here.
        runCatching {
            val token = FirebaseMessaging.getInstance().token.await()
            apiService.registerPushToken(RegisterPushTokenRequest(token = token))
        }
    }

    fun isLoggedIn(): Boolean = sessionManager.isLoggedIn()
    fun isOfflineModeActive(): Boolean = sessionManager.isOfflineModeActive()
    fun logout() = sessionManager.clearSession()
}
