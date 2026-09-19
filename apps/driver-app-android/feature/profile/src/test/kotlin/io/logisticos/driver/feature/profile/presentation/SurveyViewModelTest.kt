package io.logisticos.driver.feature.profile.presentation

import io.logisticos.driver.core.network.service.AddendumDto
import io.logisticos.driver.core.network.service.AddendumResponse
import io.logisticos.driver.core.network.service.HomeCatalogueData
import io.logisticos.driver.core.network.service.HomeCatalogueItemDto
import io.logisticos.driver.core.network.service.HomeCatalogueResponse
import io.logisticos.driver.core.network.service.HomeMoveApiService
import io.logisticos.driver.core.network.service.HomeMoveDetailResponse
import io.logisticos.driver.core.network.service.HomeMoveDto
import io.logisticos.driver.core.network.service.HomePropertyDto
import io.logisticos.driver.core.network.service.HomeRoomDto
import io.logisticos.driver.core.network.service.SurveyRequest
import io.logisticos.driver.core.network.service.SurveyResponse
import io.logisticos.driver.core.network.service.SurveyResult
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
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.BeforeEach
import org.junit.jupiter.api.Test
import retrofit2.HttpException
import retrofit2.Response

@OptIn(ExperimentalCoroutinesApi::class)
class SurveyViewModelTest {
    private val api: HomeMoveApiService = mockk()
    private val sofa = HomeCatalogueItemDto(key = "sofa_3", group = "living", name = "3-seat sofa", volumeL = 2000, weightKg = 60, assembly = true)

    @BeforeEach fun setUp() { Dispatchers.setMain(UnconfinedTestDispatcher()) }
    @AfterEach fun tearDown() { Dispatchers.resetMain() }

    private fun booked(addendum: AddendumDto? = null) {
        coEvery { api.move("s1") } returns HomeMoveDetailResponse(
            HomeMoveDto(
                shipmentId = "s1",
                property = HomePropertyDto(type = "villa", size = "3 bedroom"),
                surveyRequired = true,
                moveAt = "2026-09-26T00:00:00Z",
                totalCents = 1_500_000,
                trucks = 1,
                crewTotal = 5,
            ),
            leadName = "Idris Kamal",
        )
        coEvery { api.catalogue("villa") } returns HomeCatalogueResponse(
            HomeCatalogueData(propertyType = "villa", rooms = listOf(HomeRoomDto("living", "Living room", listOf(sofa)), HomeRoomDto("garage", "Garage"))),
        )
        coEvery { api.addendum("s1") } returns AddendumResponse(addendum)
    }

    private fun vm() = SurveyViewModel(api).also { it.open("s1") }

    @Test
    fun `loads the move and the catalogue for its property type`() = runTest {
        booked()
        val s = vm().uiState.value
        assertFalse(s.loading)
        assertEquals("Idris Kamal", s.leadName)
        assertEquals("living", s.room?.key)
        assertTrue(s.canSubmit)
    }

    @Test
    fun `sends only what was added`() = runTest {
        booked()
        val body = slot<SurveyRequest>()
        coEvery { api.submitSurvey("s1", capture(body)) } returns SurveyResponse(
            SurveyResult(addendum = AddendumDto(id = "a1", totalCents = 250_000)),
        )
        val vm = vm()
        vm.add("living", sofa)
        assertNull(vm.addExtra(ExtraDraft("material", "Wardrobe boxes", 4, 35_000)))
        vm.submit()

        assertEquals(1, body.captured.items.size)
        assertEquals("Wardrobe boxes", body.captured.extras.single().name)
        assertEquals("a1", vm.uiState.value.result?.addendum?.id)
        assertFalse(vm.uiState.value.canSubmit)
    }

    @Test
    fun `nothing is fetched until a move is opened, and reopening it refetches nothing`() = runTest {
        booked()
        val vm = SurveyViewModel(api)
        coVerify(exactly = 0) { api.move(any()) }
        vm.open("s1")
        vm.open("s1")
        coVerify(exactly = 1) { api.move("s1") }
    }

    @Test
    fun `a bad extra is refused before it is added`() = runTest {
        booked()
        val vm = vm()
        assertNotNull(vm.addExtra(ExtraDraft("material", "Boxes", 0, 100)))
        assertTrue(vm.uiState.value.extras.isEmpty())
    }

    @Test
    fun `an addendum still with the customer blocks another`() = runTest {
        booked(AddendumDto(id = "a0", totalCents = 90_000, status = "pending"))
        val vm = vm()
        assertTrue(vm.uiState.value.waitingOnCustomer)
        assertFalse(vm.uiState.value.canSubmit)
        vm.submit()
        coVerify(exactly = 0) { api.submitSurvey(any(), any()) }
    }

    @Test
    fun `a 409 reads as waiting on the customer`() = runTest {
        booked()
        coEvery { api.submitSurvey("s1", any()) } throws HttpException(
            Response.error<Any>(409, """{"error":{"code":"CONFLICT","message":"x"}}""".toResponseBody()),
        )
        val vm = vm()
        vm.submit()
        assertTrue(vm.uiState.value.submitError!!.contains("waiting on the customer"))
        assertNull(vm.uiState.value.result)
    }
}
