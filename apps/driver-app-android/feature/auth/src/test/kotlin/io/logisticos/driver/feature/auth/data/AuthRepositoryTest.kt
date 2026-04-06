package io.logisticos.driver.feature.auth.data

import io.logisticos.driver.core.network.auth.SessionManager
import io.logisticos.driver.core.network.auth.TokenStorage
import io.logisticos.driver.core.network.service.IdentityApiService
import io.logisticos.driver.core.network.service.OtpVerifyResponse
import io.mockk.coEvery
import io.mockk.mockk
import io.mockk.verify
import kotlinx.coroutines.test.runTest
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.Assertions.*

class AuthRepositoryTest {
    private val apiService: IdentityApiService = mockk()
    private val tokenStorage: TokenStorage = mockk(relaxed = true)
    private val sessionManager = SessionManager(tokenStorage)
    private val repo = AuthRepository(apiService, sessionManager)

    @Test
    fun `verifyOtp saves tokens on success`() = runTest {
        coEvery { apiService.verifyOtp(any()) } returns OtpVerifyResponse(
            jwt = "new.jwt", refreshToken = "new.refresh",
            driverId = "d-1", tenantId = "t-1"
        )
        val result = repo.verifyOtp(phone = "+639123456789", otp = "123456")
        assertTrue(result.isSuccess)
        verify { tokenStorage.saveJwt("new.jwt") }
        verify { tokenStorage.saveRefreshToken("new.refresh") }
        verify { tokenStorage.saveTenantId("t-1") }
    }

    @Test
    fun `verifyOtp returns failure on API error`() = runTest {
        coEvery { apiService.verifyOtp(any()) } throws RuntimeException("network error")
        val result = repo.verifyOtp(phone = "+639123456789", otp = "000000")
        assertTrue(result.isFailure)
    }

    @Test
    fun `sendOtp returns success on API success`() = runTest {
        coEvery { apiService.sendOtp(any()) } returns io.logisticos.driver.core.network.service.OtpSendResponse("OTP sent")
        val result = repo.sendOtp("+639123456789")
        assertTrue(result.isSuccess)
    }
}
