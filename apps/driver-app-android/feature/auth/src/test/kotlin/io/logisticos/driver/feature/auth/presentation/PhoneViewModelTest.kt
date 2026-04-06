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
class PhoneViewModelTest {
    private val testDispatcher = UnconfinedTestDispatcher()
    private val repo: AuthRepository = mockk()
    private lateinit var vm: PhoneViewModel

    @BeforeEach fun setUp() {
        Dispatchers.setMain(testDispatcher)
        vm = PhoneViewModel(repo)
    }

    @AfterEach fun tearDown() { Dispatchers.resetMain() }

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
    fun `sendOtp with short phone sets validation error`() = runTest {
        vm.onPhoneChanged("123")
        vm.uiState.test {
            awaitItem() // skip intermediate
            vm.sendOtp()
            val error = awaitItem()
            assertEquals("Enter a valid phone number", error.error)
            assertFalse(error.isLoading)
        }
    }

    @Test
    fun `sendOtp sets otpSent on success`() = runTest {
        coEvery { repo.sendOtp(any()) } returns Result.success(Unit)
        vm.onPhoneChanged("+639123456789")
        vm.uiState.test {
            awaitItem() // initial
            vm.sendOtp()
            val loading = awaitItem()
            assertTrue(loading.isLoading)
            val success = awaitItem()
            assertTrue(success.otpSent)
            assertFalse(success.isLoading)
        }
    }

    @Test
    fun `sendOtp sets error on API failure`() = runTest {
        coEvery { repo.sendOtp(any()) } returns Result.failure(RuntimeException("SMS error"))
        vm.onPhoneChanged("+639123456789")
        vm.uiState.test {
            awaitItem()
            vm.sendOtp()
            awaitItem() // loading
            val error = awaitItem()
            assertEquals("SMS error", error.error)
        }
    }
}
