package io.logisticos.driver.feature.home.presentation

import app.cash.turbine.test
import io.logisticos.driver.core.database.entity.ShiftEntity
import io.logisticos.driver.feature.home.data.ShiftRepository
import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.test.*
import org.junit.jupiter.api.*
import org.junit.jupiter.api.Assertions.*

@OptIn(ExperimentalCoroutinesApi::class)
class HomeViewModelTest {
    private val testDispatcher = UnconfinedTestDispatcher()
    private val repo: ShiftRepository = mockk()
    private lateinit var vm: HomeViewModel

    @BeforeEach fun setUp() {
        Dispatchers.setMain(testDispatcher)
        val shift = ShiftEntity("s1", "d1", "t1", null, null, true, 5, 2, 0, 0.0, null)
        every { repo.observeActiveShift() } returns flowOf(shift)
        coEvery { repo.syncShift() } returns Unit
        vm = HomeViewModel(repo)
    }

    @AfterEach fun tearDown() { Dispatchers.resetMain() }

    @Test
    fun `shift is loaded from repository`() = runTest {
        vm.uiState.test {
            val state = awaitItem()
            assertNotNull(state.shift)
            assertEquals(5, state.shift?.totalStops)
        }
    }
}
