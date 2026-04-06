package io.logisticos.driver.feature.auth.presentation

import app.cash.turbine.test
import io.logisticos.driver.feature.auth.data.AuthRepository
import io.mockk.coEvery
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

    @BeforeEach fun setUp() {
        Dispatchers.setMain(testDispatcher)
        vm = OtpViewModel(repo)
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
        coEvery { repo.verifyOtp(any(), any()) } returns Result.success(Unit)
        vm.uiState.test {
            awaitItem() // initial
            vm.verifyOtp(phone = "+639123456789", otp = "123456")
            val loading = awaitItem()
            assertTrue(loading.isLoading)
            val success = awaitItem()
            assertTrue(success.isSuccess)
            assertFalse(success.isLoading)
        }
    }

    @Test
    fun `verifyOtp sets error on failure`() = runTest {
        coEvery { repo.verifyOtp(any(), any()) } returns Result.failure(RuntimeException("Invalid OTP"))
        vm.uiState.test {
            awaitItem()
            vm.verifyOtp(phone = "+639123456789", otp = "000000")
            awaitItem() // loading
            val error = awaitItem()
            assertEquals("Invalid OTP", error.error)
            assertFalse(error.isLoading)
        }
    }

    @Test
    fun `onOtpChanged ignores input longer than 6 chars`() = runTest {
        vm.onOtpChanged("1234567")
        vm.uiState.test {
            val state = awaitItem()
            assertEquals("", state.otp) // unchanged — was never set to 1234567
        }
    }
}
