package io.logisticos.driver.feature.profile.presentation

import io.logisticos.driver.core.network.service.AvailabilityData
import io.logisticos.driver.core.network.service.AvailabilityRequest
import io.logisticos.driver.core.network.service.AvailabilityResponse
import io.logisticos.driver.core.network.service.HomeMoveApiService
import io.logisticos.driver.core.network.service.ProviderData
import io.logisticos.driver.core.network.service.ProviderProfileDto
import io.logisticos.driver.core.network.service.ProviderResponse
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import io.mockk.slot
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import okhttp3.ResponseBody.Companion.toResponseBody
import org.junit.jupiter.api.AfterEach
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertNotNull
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.BeforeEach
import org.junit.jupiter.api.Test
import retrofit2.HttpException
import retrofit2.Response
import java.time.LocalDate

@OptIn(ExperimentalCoroutinesApi::class)
class AvailabilityViewModelTest {
    private val api: HomeMoveApiService = mockk()
    private val today = LocalDate.of(2026, 9, 19)

    @BeforeEach fun setUp() { Dispatchers.setMain(UnconfinedTestDispatcher()) }
    @AfterEach fun tearDown() { Dispatchers.resetMain() }

    private fun lead() {
        coEvery { api.myProvider() } returns ProviderResponse(
            ProviderData(
                ProviderProfileDto(serviceLines = listOf("home_move"), coverage = listOf("local"), fleetTrucks = 2, registeredHelpers = 6, workingDays = listOf(5, 0, 1)),
                offDays = listOf("2026-10-02"),
            ),
        )
    }

    @Test
    fun `loads the lead's days in order`() = runTest {
        lead()
        val s = AvailabilityViewModel(api).uiState.value
        assertTrue(s.isHomeLead)
        assertEquals(listOf(0, 1, 5), s.workingDays)
        assertFalse(s.dirty)
    }

    @Test
    fun `saves both lists whole`() = runTest {
        lead()
        val body = slot<AvailabilityRequest>()
        coEvery { api.setAvailability(capture(body)) } answers {
            AvailabilityResponse(AvailabilityData(body.captured.workingDays, body.captured.offDays))
        }
        val vm = AvailabilityViewModel(api)
        vm.toggle(5)
        vm.addOff(LocalDate.of(2026, 9, 25), today)
        vm.save()

        assertEquals(listOf(0, 1), body.captured.workingDays)
        assertEquals(listOf("2026-09-25", "2026-10-02"), body.captured.offDays)
        assertTrue(vm.uiState.value.saved)
        assertFalse(vm.uiState.value.dirty)
    }

    @Test
    fun `a past day off is refused and nothing is sent unchanged`() = runTest {
        lead()
        val vm = AvailabilityViewModel(api)
        vm.addOff(today.minusDays(3), today)
        assertNotNull(vm.uiState.value.notice)
        assertFalse(vm.uiState.value.dirty)
        vm.save()
        coVerify(exactly = 0) { api.setAvailability(any()) }
    }

    @Test
    fun `a driver not onboarded is told so`() = runTest {
        coEvery { api.myProvider() } throws HttpException(Response.error<Any>(404, "{}".toResponseBody()))
        val s = AvailabilityViewModel(api).uiState.value
        assertTrue(s.error!!.contains("onboarded"))
    }
}
