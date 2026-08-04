package io.logisticos.driver.feature.pod.presentation

import app.cash.turbine.test
import io.logisticos.driver.core.database.entity.TaskEntity
import io.logisticos.driver.core.database.entity.TaskStatus
import io.logisticos.driver.core.location.LatLng
import io.logisticos.driver.core.location.LocationRepository
import io.logisticos.driver.feature.delivery.data.DeliveryRepository
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.mockk.slot
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.jupiter.api.AfterEach
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertNotEquals
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.BeforeEach
import org.junit.jupiter.api.Test

@OptIn(ExperimentalCoroutinesApi::class)
class PodViewModelTest {

    private val testDispatcher = UnconfinedTestDispatcher()
    private val repo: DeliveryRepository = mockk(relaxed = true)
    private val locationRepo: LocationRepository = mockk(relaxed = true)
    private lateinit var vm: PodViewModel

    /** Delivery address stored on the task — the geofence anchor. */
    private val deliveryLat = 14.5995
    private val deliveryLng = 120.9842

    /** Where the driver actually is — deliberately ~1 km from the address so a
     *  self-comparison bug shows up as a 0 m distance instead of a real one. */
    private val driverLat = 14.6085
    private val driverLng = 120.9842

    private fun task() = TaskEntity(
        id = "t1",
        shipmentId = "s1",
        awb = "CM-PH1-S0001234X",
        recipientName = "Ana Cruz",
        recipientPhone = "+639170000000",
        address = "123 Rizal Ave",
        lat = deliveryLat,
        lng = deliveryLng,
        status = TaskStatus.IN_PROGRESS,
        stopOrder = 1,
        syncedAt = null,
    )

    @BeforeEach
    fun setUp() {
        Dispatchers.setMain(testDispatcher)
        // observeTask is a plain function returning a Flow, not a suspend fun.
        every { repo.observeTask("t1") } returns flowOf(task())
        vm = PodViewModel(repo, locationRepo)
        vm.setRequirements(
            taskId = "t1",
            shipmentId = "s1",
            recipientName = "Ana Cruz",
            requiresPhoto = true,
            requiresSignature = true,
            requiresOtp = false,
        )
    }

    @AfterEach
    fun tearDown() {
        Dispatchers.resetMain()
    }

    @Test
    fun `canSubmit is false when photo not yet captured`() = runTest {
        vm.uiState.test {
            assertFalse(awaitItem().canSubmit)
        }
    }

    @Test
    fun `canSubmit is true when all required evidence is present`() = runTest {
        vm.uiState.test {
            awaitItem()
            vm.onPhotoCaptured("/path/photo.jpg")
            awaitItem()
            vm.onSignatureSaved("/path/sig.png")
            assertTrue(awaitItem().canSubmit)
        }
    }

    @Test
    fun `submit sends the driver GPS as capture and the address as delivery`() = runTest {
        // Regression guard: submitPod used to receive the capture point for both
        // pairs, which reduced the server-side geofence to a self-comparison that
        // always measured 0 m and could never fail.
        vm.loadTaskMeta("t1")
        coEvery { locationRepo.getLastKnownLocation() } returns LatLng(driverLat, driverLng)

        val capLat = slot<Double>()
        val capLng = slot<Double>()
        val delLat = slot<Double>()
        val delLng = slot<Double>()
        coEvery {
            repo.submitPod(
                taskId = any(), shipmentId = any(), recipientName = any(),
                captureLat = capture(capLat), captureLng = capture(capLng),
                deliveryLat = capture(delLat), deliveryLng = capture(delLng),
                photoPath = any(), signaturePath = any(), otpCode = any(),
                codCollectedCents = any(), deviceTimestamp = any(),
                requiresPhoto = any(), requiresSignature = any(),
            )
        } returns "pod-1"

        vm.onPhotoCaptured("/path/photo.jpg")
        vm.onSignatureSaved("/path/sig.png")
        vm.submit("t1")

        assertEquals(driverLat, capLat.captured)
        assertEquals(driverLng, capLng.captured)
        assertEquals(deliveryLat, delLat.captured)
        assertEquals(deliveryLng, delLng.captured)
        assertNotEquals(capLat.captured, delLat.captured)
    }

    @Test
    fun `submit sends an ISO-8601 device timestamp`() = runTest {
        vm.loadTaskMeta("t1")
        coEvery { locationRepo.getLastKnownLocation() } returns LatLng(driverLat, driverLng)

        val deviceTs = slot<String>()
        coEvery {
            repo.submitPod(
                taskId = any(), shipmentId = any(), recipientName = any(),
                captureLat = any(), captureLng = any(),
                deliveryLat = any(), deliveryLng = any(),
                photoPath = any(), signaturePath = any(), otpCode = any(),
                codCollectedCents = any(), deviceTimestamp = capture(deviceTs),
                requiresPhoto = any(), requiresSignature = any(),
            )
        } returns "pod-1"

        vm.onPhotoCaptured("/path/photo.jpg")
        vm.onSignatureSaved("/path/sig.png")
        vm.submit("t1")

        // Instant.parse throws if this is not a valid ISO-8601 instant.
        java.time.Instant.parse(deviceTs.captured)
    }

    @Test
    fun `submit falls back to the delivery address when there is no GPS fix`() = runTest {
        vm.loadTaskMeta("t1")
        coEvery { locationRepo.getLastKnownLocation() } returns null

        val capLat = slot<Double>()
        coEvery {
            repo.submitPod(
                taskId = any(), shipmentId = any(), recipientName = any(),
                captureLat = capture(capLat), captureLng = any(),
                deliveryLat = any(), deliveryLng = any(),
                photoPath = any(), signaturePath = any(), otpCode = any(),
                codCollectedCents = any(), deviceTimestamp = any(),
                requiresPhoto = any(), requiresSignature = any(),
            )
        } returns "pod-1"

        vm.onPhotoCaptured("/path/photo.jpg")
        vm.onSignatureSaved("/path/sig.png")
        vm.submit("t1")

        // Documented degradation: without a fix the geofence is not meaningful,
        // but the delivery must not be blocked.
        assertEquals(deliveryLat, capLat.captured)
    }

    @Test
    fun `submit is a no-op until required evidence is captured`() = runTest {
        vm.submit("t1")
        coVerify(exactly = 0) {
            repo.submitPod(
                taskId = any(), shipmentId = any(), recipientName = any(),
                captureLat = any(), captureLng = any(),
                deliveryLat = any(), deliveryLng = any(),
                photoPath = any(), signaturePath = any(), otpCode = any(),
                codCollectedCents = any(), deviceTimestamp = any(),
                requiresPhoto = any(), requiresSignature = any(),
            )
        }
    }
}
