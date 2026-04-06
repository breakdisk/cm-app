package io.logisticos.driver.feature.pod.presentation

import app.cash.turbine.test
import io.logisticos.driver.feature.delivery.data.DeliveryRepository
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.jupiter.api.AfterEach
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.BeforeEach
import org.junit.jupiter.api.Test

@OptIn(ExperimentalCoroutinesApi::class)
class PodViewModelTest {
    private val testDispatcher = UnconfinedTestDispatcher()
    private val repo: DeliveryRepository = mockk(relaxed = true)
    private lateinit var vm: PodViewModel

    @BeforeEach
    fun setUp() {
        Dispatchers.setMain(testDispatcher)
        vm = PodViewModel(repo)
        vm.setRequirements(taskId = "t1", requiresPhoto = true, requiresSignature = true, requiresOtp = false)
    }

    @AfterEach
    fun tearDown() {
        Dispatchers.resetMain()
    }

    @Test
    fun `canSubmit is false when photo not yet captured`() = runTest {
        vm.uiState.test {
            val state = awaitItem()
            assertFalse(state.canSubmit)
        }
    }

    @Test
    fun `canSubmit is true when all required steps done`() = runTest {
        vm.uiState.test {
            awaitItem()
            vm.onPhotoCaptured("/path/photo.jpg")
            awaitItem()
            vm.onSignatureSaved("/path/sig.png")
            val state = awaitItem()
            assertTrue(state.canSubmit)
        }
    }

    @Test
    fun `submit triggers savePod in repository`() = runTest {
        coEvery { repo.savePod(any(), any(), any(), any()) } returns Unit
        vm.onPhotoCaptured("/path/photo.jpg")
        vm.onSignatureSaved("/path/sig.png")
        vm.submit()
        coVerify { repo.savePod("t1", any(), any(), any()) }
    }
}
