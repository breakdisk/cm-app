package io.logisticos.driver.feature.auth.presentation

import app.cash.turbine.test
import io.logisticos.driver.core.network.auth.SessionManager
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
class PhoneViewModelTest {
    private val testDispatcher = UnconfinedTestDispatcher()
    private val repo: AuthRepository = mockk()
    private val sessionManager: SessionManager = mockk(relaxed = true)
    private lateinit var vm: PhoneViewModel

    private val phone = "+639123456789"
    private val slug = "demo"

    @BeforeEach fun setUp() {
        Dispatchers.setMain(testDispatcher)
        vm = PhoneViewModel(repo, sessionManager)
    }

    @AfterEach fun tearDown() { Dispatchers.resetMain() }

    /** sendOtp() refuses to run without a tenant, so most tests need this first. */
    private fun applySlug() {
        vm.onCompanyCodeChanged(slug)
        vm.applyCompanyCode()
    }

    @Test
    fun `initial state has empty phone and no error`() = runTest {
        vm.uiState.test {
            val state = awaitItem()
            assertEquals("", state.phone)
            assertNull(state.error)
            assertFalse(state.isLoading)
            assertFalse(state.otpSent)
        }
    }

    @Test
    fun `sendOtp without a company code asks for one`() = runTest {
        // The tenant slug is what scopes the whole login; dispatching an OTP
        // without it would look up the driver in no tenant at all.
        vm.onPhoneChanged(phone)
        vm.uiState.test {
            awaitItem()
            vm.sendOtp()
            assertEquals("Enter your company code to continue", awaitItem().error)
        }
    }

    @Test
    fun `applyCompanyCode rejects a blank code`() = runTest {
        vm.onCompanyCodeChanged("   ")
        vm.uiState.test {
            awaitItem()
            vm.applyCompanyCode()
            assertEquals("Enter your company code", awaitItem().companyCodeError)
        }
    }

    @Test
    fun `sendOtp with short phone sets validation error`() = runTest {
        applySlug()
        vm.onPhoneChanged("123")
        vm.uiState.test {
            awaitItem()
            vm.sendOtp()
            val error = awaitItem()
            assertEquals("Enter a valid phone number", error.error)
            assertFalse(error.isLoading)
        }
    }

    @Test
    fun `sendOtp sets otpSent on success`() = runTest {
        coEvery { repo.sendOtp(phone = phone, tenantSlug = slug) } returns Result.success(Unit)
        applySlug()
        vm.onPhoneChanged(phone)
        vm.uiState.test {
            awaitItem()
            vm.sendOtp()
            val success = awaitItem()
            assertTrue(success.otpSent)
            assertFalse(success.isLoading)
        }
    }

    @Test
    fun `sendOtp sets error on API failure`() = runTest {
        coEvery { repo.sendOtp(phone = phone, tenantSlug = slug) } returns
            Result.failure(RuntimeException("SMS error"))
        applySlug()
        vm.onPhoneChanged(phone)
        vm.uiState.test {
            awaitItem()
            vm.sendOtp()
            assertEquals("SMS error", awaitItem().error)
        }
    }

    @Test
    fun `email mode sends the email rather than the phone`() = runTest {
        val email = "driver@demo.com"
        coEvery { repo.sendOtp(email = email, tenantSlug = slug) } returns Result.success(Unit)
        applySlug()
        vm.onToggleMode(emailMode = true)
        vm.onEmailChanged(email)

        vm.sendOtp()

        coVerify(exactly = 1) { repo.sendOtp(email = email, tenantSlug = slug) }
    }

    @Test
    fun `email mode rejects an address with no at sign`() = runTest {
        applySlug()
        vm.onToggleMode(emailMode = true)
        vm.onEmailChanged("not-an-email")
        vm.uiState.test {
            awaitItem()
            vm.sendOtp()
            assertEquals("Enter a valid email address", awaitItem().error)
        }
    }
}
