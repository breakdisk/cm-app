package io.logisticos.driver.feature.profile.presentation

import io.logisticos.driver.core.network.service.DriverOpsApiService
import io.logisticos.driver.core.network.service.HosData
import io.logisticos.driver.core.network.service.HosResponse
import io.mockk.coEvery
import io.mockk.mockk
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.jupiter.api.AfterEach
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.BeforeEach
import org.junit.jupiter.api.Test
import java.io.IOException

@OptIn(ExperimentalCoroutinesApi::class)
class HosViewModelTest {
    private val api: DriverOpsApiService = mockk()

    @BeforeEach fun setUp() { Dispatchers.setMain(UnconfinedTestDispatcher()) }
    @AfterEach fun tearDown() { Dispatchers.resetMain() }

    @Test
    fun `nothing is fetched until a screen asks`() = runTest {
        val vm = HosViewModel(api)
        assertNull(vm.uiState.value.clock)
        assertFalse(vm.uiState.value.loading)
    }

    @Test
    fun `shows the clock driver-ops computed`() = runTest {
        coEvery { api.getMyHos() } returns HosResponse(
            HosData(onDutyMinutes = 372, remainingMinutes = 288, onDutySince = "2026-09-15T06:00:00Z", currentStretchMinutes = 372)
        )
        val vm = HosViewModel(api)
        vm.refresh()

        val state = vm.uiState.value
        assertFalse(state.loading)
        assertEquals(372L, state.clock?.onDutyMinutes)
        assertEquals(660L, state.clock?.limitMinutes)
        assertTrue(state.fetchedAtMillis > 0L)
    }

    @Test
    fun `shows no clock when driver-ops cannot give one`() = runTest {
        coEvery { api.getMyHos() } throws IOException("HTTP 404")
        val vm = HosViewModel(api)
        vm.refresh()

        assertNull(vm.uiState.value.clock)
        assertFalse(vm.uiState.value.loading)
    }

    @Test
    fun `keeps the last clock when a later refresh fails`() = runTest {
        coEvery { api.getMyHos() } returns HosResponse(HosData(onDutyMinutes = 60)) andThenThrows IOException("offline")
        val vm = HosViewModel(api)
        vm.refresh()
        vm.refresh()

        assertEquals(60L, vm.uiState.value.clock?.onDutyMinutes)
        assertFalse(vm.uiState.value.loading)
    }
}
