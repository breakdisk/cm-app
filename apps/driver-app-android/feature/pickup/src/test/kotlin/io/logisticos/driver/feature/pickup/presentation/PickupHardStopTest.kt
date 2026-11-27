package io.logisticos.driver.feature.pickup.presentation

import io.logisticos.driver.core.database.entity.TaskEntity
import io.logisticos.driver.core.database.entity.TaskStatus
import io.logisticos.driver.core.database.entity.TaskType
import io.logisticos.driver.core.location.LocationRepository
import io.logisticos.driver.core.network.service.AddendumDto
import io.logisticos.driver.core.network.service.AddendumResponse
import io.logisticos.driver.core.network.service.HomeMoveApiService
import io.logisticos.driver.feature.pickup.data.PickupRepository
import io.mockk.coEvery
import io.mockk.every
import io.mockk.just
import io.mockk.mockk
import io.mockk.runs
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import okhttp3.ResponseBody.Companion.toResponseBody
import org.junit.jupiter.api.AfterEach
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertNotNull
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.BeforeEach
import org.junit.jupiter.api.Test
import retrofit2.HttpException
import retrofit2.Response

/** A whole-home move's pickup can't be closed while a survey addition waits on the customer. */
@OptIn(ExperimentalCoroutinesApi::class)
class PickupHardStopTest {
    private val repo: PickupRepository = mockk()
    private val location: LocationRepository = mockk(relaxed = true)
    private val home: HomeMoveApiService = mockk()

    private val task = TaskEntity(
        id = "t1", shipmentId = "s1", taskType = TaskType.PICKUP, awb = "", recipientName = "Ana",
        recipientPhone = "", address = "12 Acacia St", status = TaskStatus.ASSIGNED, stopOrder = 1, syncedAt = null,
    )

    @BeforeEach fun setUp() {
        Dispatchers.setMain(UnconfinedTestDispatcher())
        every { repo.observeTask("t1") } returns flowOf(task)
        coEvery { repo.transitionToInProgress("t1") } just runs
    }

    @AfterEach fun tearDown() { Dispatchers.resetMain() }

    @Test
    fun `an addition waiting on the customer blocks the pickup`() = runTest {
        coEvery { home.addendum("s1") } returns AddendumResponse(AddendumDto(id = "a1", totalCents = 250_000, status = "pending"))
        val vm = PickupViewModel(repo, location, home)
        vm.load("t1")
        assertNotNull(vm.uiState.value.addendumHold)
        assertFalse(vm.uiState.value.canConfirm)
    }

    @Test
    fun `an answered addition does not`() = runTest {
        coEvery { home.addendum("s1") } returns AddendumResponse(AddendumDto(id = "a1", status = "declined"))
        val vm = PickupViewModel(repo, location, home)
        vm.load("t1")
        assertNull(vm.uiState.value.addendumHold)
        assertTrue(vm.uiState.value.canConfirm)
    }

    @Test
    fun `a shipment that is not a home move is never held`() = runTest {
        coEvery { home.addendum("s1") } throws HttpException(Response.error<Any>(404, "{}".toResponseBody()))
        val vm = PickupViewModel(repo, location, home)
        vm.load("t1")
        assertNull(vm.uiState.value.addendumHold)
        assertTrue(vm.uiState.value.canConfirm)
    }
}
