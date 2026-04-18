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
    @Named("tenant_slug") private val tenantSlug: String
) {
    suspend fun sendOtp(phone: String): Result<Unit> = runCatching {
        apiService.sendOtp(OtpSendRequest(phone = phone, tenantSlug = tenantSlug, role = "driver"))
        Unit
    }

    suspend fun verifyOtp(phone: String, otp: String): Result<Unit> = runCatching {
        val response = apiService.verifyOtp(
            OtpVerifyRequest(phone = phone, otp = otp, tenantSlug = tenantSlug, role = "driver")
        ).data
        sessionManager.saveTokens(jwt = response.jwt, refreshToken = response.refreshToken)
        sessionManager.saveTenantId(response.tenantId)
        sessionManager.saveDriverId(response.driverId)
    }

    fun isLoggedIn(): Boolean = sessionManager.isLoggedIn()
    fun isOfflineModeActive(): Boolean = sessionManager.isOfflineModeActive()
    fun logout() = sessionManager.clearSession()
}
