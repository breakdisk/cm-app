package io.logisticos.driver.feature.auth.presentation

import app.cash.turbine.test
import io.logisticos.driver.feature.auth.data.AuthRepository
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.*
import org.junit.jupiter.api.AfterEach
import org.junit.jupiter.api.BeforeEach
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.Assertions.*

@OptIn(ExperimentalCoroutinesApi::class)
class OtpViewModelTest {
    private val testDispatcher = UnconfinedTestDispatcher()
    private val repo: AuthRepository = mockk()
    private lateinit var vm: OtpViewModel

    private val phone = "+639123456789"
    private val slug = "demo"

    @BeforeEach fun setUp() {
        Dispatchers.setMain(testDispatcher)
        vm = OtpViewModel(repo)
        // verifyOtp/resendOtp read the slug off state; without it every call
        // would go out with an empty tenant.
        vm.setTenantSlug(slug)
    }

    @AfterEach fun tearDown() { Dispatchers.resetMain() }

    @Test
    fun `initial state is idle`() = runTest {
        vm.uiState.test {
            val state = awaitItem()
            assertFalse(state.isLoading)
            assertNull(state.error)
            assertFalse(state.isSuccess)
        }
    }

    @Test
    fun `verifyOtp sets isSuccess on success`() = runTest {
        coEvery { repo.verifyOtp(phone = phone, otp = "123456", tenantSlug = slug) } returns Result.success(Unit)
        vm.uiState.test {
            awaitItem() // initial
            vm.verifyOtp(identifier = phone, otp = "123456")
            val success = awaitItem()
            assertTrue(success.isSuccess)
            assertFalse(success.isLoading)
        }
    }

    @Test
    fun `verifyOtp sets error on failure`() = runTest {
        coEvery { repo.verifyOtp(phone = phone, otp = "000000", tenantSlug = slug) } returns
            Result.failure(RuntimeException("Invalid OTP"))
        vm.uiState.test {
            awaitItem()
            vm.verifyOtp(identifier = phone, otp = "000000")
            val error = awaitItem()
            assertEquals("Invalid OTP", error.error)
            assertFalse(error.isLoading)
        }
    }

    @Test
    fun `verifyOtp uses default error message when exception has no message`() = runTest {
        coEvery { repo.verifyOtp(phone = phone, otp = "000000", tenantSlug = slug) } returns
            Result.failure(RuntimeException())
        vm.uiState.test {
            awaitItem()
            vm.verifyOtp(identifier = phone, otp = "000000")
            assertEquals("Invalid OTP", awaitItem().error)
        }
    }

    @Test
    fun `verifyOtp routes an email identifier to the email parameter`() = runTest {
        // The identifier is a single field in the UI; the "@" test is what decides
        // whether it is sent as a phone or an email. Sending an email as a phone
        // would fail server-side lookup with a confusing "OTP invalid".
        val email = "driver@demo.com"
        coEvery { repo.verifyOtp(email = email, otp = "123456", tenantSlug = slug) } returns Result.success(Unit)

        vm.verifyOtp(identifier = email, otp = "123456")

        coVerify(exactly = 1) { repo.verifyOtp(email = email, otp = "123456", tenantSlug = slug) }
    }

    @Test
    fun `resendOtp routes a phone identifier to the phone parameter`() = runTest {
        coEvery { repo.sendOtp(phone = phone, tenantSlug = slug) } returns Result.success(Unit)

        vm.resendOtp(identifier = phone)

        coVerify(exactly = 1) { repo.sendOtp(phone = phone, tenantSlug = slug) }
    }

    @Test
    fun `onOtpChanged ignores input longer than 6 chars`() = runTest {
        vm.onOtpChanged("1234567")
        vm.uiState.test {
            assertEquals("", awaitItem().otp) // never accepted
        }
    }
}
