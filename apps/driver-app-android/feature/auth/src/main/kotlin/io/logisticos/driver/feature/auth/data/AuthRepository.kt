package io.logisticos.driver.feature.auth.data

import io.logisticos.driver.core.network.auth.SessionManager
import io.logisticos.driver.core.network.service.IdentityApiService
import io.logisticos.driver.core.network.service.OtpSendRequest
import io.logisticos.driver.core.network.service.OtpVerifyRequest
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
    suspend fun sendOtp(phone: String? = null, email: String? = null): Result<Unit> {
        // runCatching is intentionally avoided here: it catches CancellationException,
        // breaking structured concurrency and leaving isLoading=true if the coroutine
        // is cancelled mid-flight (e.g. screen rotation while the call is in flight).
        return try {
            apiService.sendOtp(
                OtpSendRequest(phone = phone, email = email, tenantSlug = tenantSlug, role = "driver")
            )
            Result.success(Unit)
        } catch (e: kotlinx.coroutines.CancellationException) {
            throw e   // must not swallow — propagate so structured concurrency works
        } catch (e: Exception) {
            Result.failure(e)
        }
    }

    suspend fun verifyOtp(phone: String? = null, otp: String, email: String? = null): Result<Unit> {
        if (devBypassEnabled && otp == "123456") {
            sessionManager.saveTokens(jwt = "dev-token", refreshToken = "dev-refresh")
            sessionManager.saveTenantId(tenantSlug)   // use real tenant, not hardcoded literal
            sessionManager.saveDriverId("dev-driver-id")
            return Result.success(Unit)
        }
        return try {
            val response = apiService.verifyOtp(
                OtpVerifyRequest(phone = phone, email = email, otp = otp, tenantSlug = tenantSlug, role = "driver")
            ).data
            sessionManager.saveTokens(jwt = response.jwt, refreshToken = response.refreshToken)
            sessionManager.saveTenantId(response.tenantId)
            sessionManager.saveDriverId(response.driverId)
            // FCM token registration is handled by MainViewModel.onAuthSuccess() via
            // addOnSuccessListener (non-blocking). Do NOT await it here — Firebase token
            // fetch can stall indefinitely on first launch and would keep isLoading=true.
            Result.success(Unit)
        } catch (e: kotlinx.coroutines.CancellationException) {
            throw e
        } catch (e: Exception) {
            Result.failure(e)
        }
    }

    fun isLoggedIn(): Boolean = sessionManager.isLoggedIn()
    fun isOfflineModeActive(): Boolean = sessionManager.isOfflineModeActive()
    fun logout() = sessionManager.clearSession()
}
