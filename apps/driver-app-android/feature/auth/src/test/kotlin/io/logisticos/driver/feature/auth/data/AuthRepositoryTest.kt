package io.logisticos.driver.feature.auth.data

import io.logisticos.driver.core.network.auth.SessionManager
import io.logisticos.driver.core.network.service.ApiResponse
import io.logisticos.driver.core.network.service.DriverOpsApiService
import io.logisticos.driver.core.network.service.IdentityApiService
import io.logisticos.driver.core.network.service.OtpSendResponse
import io.logisticos.driver.core.network.service.OtpVerifyResponse
import io.mockk.coEvery
import io.mockk.mockk
import io.mockk.verify
import kotlinx.coroutines.test.runTest
import okhttp3.OkHttpClient
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.Assertions.*

class AuthRepositoryTest {
    private val apiService: IdentityApiService = mockk()
    private val driverOpsApi: DriverOpsApiService = mockk(relaxed = true)
    private val sessionManager: SessionManager = mockk(relaxed = true)
    private val okHttpClient: OkHttpClient = mockk(relaxed = true)
    private val repo = AuthRepository(
        apiService      = apiService,
        driverOpsApi    = driverOpsApi,
        sessionManager  = sessionManager,
        okHttpClient    = okHttpClient,
        devBypassEnabled = false,
    )

    @Test
    fun `verifyOtp saves tokens on success`() = runTest {
        coEvery { apiService.verifyOtp(any()) } returns ApiResponse(
            OtpVerifyResponse(
                jwt = "new.jwt", refreshToken = "new.refresh",
                driverId = "d-1", tenantId = "t-1"
            )
        )
        // getMe() is called in profile hydration — let it fail silently via runCatching
        coEvery { apiService.getMe() } throws RuntimeException("no stub needed")
        val result = repo.verifyOtp(phone = "+639123456789", otp = "123456", tenantSlug = "demo")
        assertTrue(result.isSuccess)
        verify { sessionManager.saveTokens("new.jwt", "new.refresh") }
        verify { sessionManager.saveTenantId("t-1") }
    }

    @Test
    fun `verifyOtp returns failure on API error`() = runTest {
        coEvery { apiService.verifyOtp(any()) } throws RuntimeException("network error")
        val result = repo.verifyOtp(phone = "+639123456789", otp = "000000", tenantSlug = "demo")
        assertTrue(result.isFailure)
    }

    @Test
    fun `sendOtp returns success on API success`() = runTest {
        coEvery { apiService.sendOtp(any()) } returns ApiResponse(OtpSendResponse("OTP sent"))
        val result = repo.sendOtp(phone = "+639123456789", tenantSlug = "demo")
        assertTrue(result.isSuccess)
    }

    @Test
    fun `verifyOtp dev bypass skips network call and saves placeholder tokens`() = runTest {
        val bypassRepo = AuthRepository(
            apiService      = apiService,
            driverOpsApi    = driverOpsApi,
            sessionManager  = sessionManager,
            okHttpClient    = okHttpClient,
            devBypassEnabled = true,
        )
        val result = bypassRepo.verifyOtp(phone = "+639123456789", otp = "123456", tenantSlug = "demo")
        assertTrue(result.isSuccess)
        verify { sessionManager.saveTokens("dev-token", "dev-refresh") }
    }
}
