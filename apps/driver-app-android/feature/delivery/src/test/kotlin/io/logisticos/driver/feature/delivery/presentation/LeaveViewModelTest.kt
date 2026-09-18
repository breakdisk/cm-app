package io.logisticos.driver.feature.delivery.presentation

import io.logisticos.driver.core.network.service.DriverOpsApiService
import io.logisticos.driver.core.network.service.LeaveQuoteData
import io.logisticos.driver.core.network.service.LeaveQuoteResponse
import io.logisticos.driver.core.network.service.LeaveTaskRequest
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
import okhttp3.MediaType
import okhttp3.ResponseBody
import org.junit.jupiter.api.AfterEach
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertNotNull
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.BeforeEach
import org.junit.jupiter.api.Test
import retrofit2.HttpException
import retrofit2.Response

@OptIn(ExperimentalCoroutinesApi::class)
class LeaveViewModelTest {
    private val api: DriverOpsApiService = mockk(relaxed = true)
    private val task = "task-1"

    private fun quote(mode: String?, refusal: String? = null) = LeaveQuoteResponse(
        LeaveQuoteData(
            mode = mode,
            refusal = refusal,
            asOf = "2026-09-18T10:00:00Z",
            feeCents = if (mode == "drop") 4_280 else 0,
            payoutCents = 21_400,
            feePct = 20,
            graceExpiresAt = "2026-09-18T10:45:00Z",
        )
    )

    @BeforeEach fun setUp() { Dispatchers.setMain(UnconfinedTestDispatcher()) }
    @AfterEach fun tearDown() { Dispatchers.resetMain() }

    @Test
    fun `the quote is what the server said`() = runTest {
        coEvery { api.getLeaveQuote(task) } returns quote("drop")
        val vm = LeaveViewModel(api)
        vm.refresh(task)
        assertEquals("drop", vm.uiState.value.quote?.mode)
        assertEquals(4_280L, vm.uiState.value.quote?.feeCents)
    }

    @Test
    fun `nothing is sent without a reason`() = runTest {
        coEvery { api.getLeaveQuote(task) } returns quote("drop")
        val vm = LeaveViewModel(api)
        vm.refresh(task)
        vm.confirm(task, null, null)
        assertEquals("Pick a reason first.", vm.uiState.value.error)
        coVerify(exactly = 0) { api.leaveTask(any(), any()) }
    }

    @Test
    fun `a confirmed leave sends the reason and note and finishes`() = runTest {
        coEvery { api.getLeaveQuote(task) } returns quote("drop")
        val sent = slot<LeaveTaskRequest>()
        coEvery { api.leaveTask(task, capture(sent)) } returns quote("drop")
        val vm = LeaveViewModel(api)
        vm.refresh(task)
        vm.pickReason("VEHICLE_TROUBLE")
        vm.setNote("  flat tyre  ")
        vm.confirm(task, 14.55, 121.02)

        assertEquals("VEHICLE_TROUBLE", sent.captured.reasonCode)
        assertEquals("flat tyre", sent.captured.note)
        assertEquals(14.55, sent.captured.lat)
        assertNotNull(vm.uiState.value.applied)
    }

    /** Grace ran out while the driver read the sheet: the server's answer
     *  is now a release, and the drop reason no longer applies. */
    @Test
    fun `a mode that flips drops a reason from the other list`() = runTest {
        coEvery { api.getLeaveQuote(task) } returns quote("drop")
        val vm = LeaveViewModel(api)
        vm.refresh(task)
        vm.pickReason("VEHICLE_TROUBLE")

        coEvery { api.getLeaveQuote(task) } returns quote("release")
        vm.refresh(task)
        assertNull(vm.uiState.value.reason)
    }

    @Test
    fun `a refusal reads as plain words and the quote is refreshed`() = runTest {
        coEvery { api.getLeaveQuote(task) } returns quote("drop")
        val body = """{"error":{"code":"BUSINESS_RULE_VIOLATION","message":"Business rule violation: GOODS_ABOARD"}}"""
        coEvery { api.leaveTask(task, any()) } throws HttpException(Response.error<Any>(422, ResponseBody.create(null as MediaType?, body)))
        val vm = LeaveViewModel(api)
        vm.refresh(task)
        vm.pickReason("VEHICLE_TROUBLE")
        vm.confirm(task, null, null)

        assertTrue(vm.uiState.value.error!!.startsWith("The load is aboard"))
        assertNull(vm.uiState.value.applied)
        coVerify(atLeast = 2) { api.getLeaveQuote(task) }
    }
}
