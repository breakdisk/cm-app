package io.logisticos.driver.feature.delivery.presentation

import io.logisticos.driver.core.network.service.EngagementApiService
import io.logisticos.driver.core.network.service.JobCallData
import io.logisticos.driver.core.network.service.JobCallResponse
import io.mockk.coEvery
import io.mockk.mockk
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.runTest
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test
import java.io.IOException

@OptIn(ExperimentalCoroutinesApi::class)
class ChatViewModelPlaceCallTest {
    private val api: EngagementApiService = mockk(relaxed = true)
    private val shipment = "ship-1"

    @Test
    fun `a bridged call is reported as bridged`() = runTest {
        coEvery { api.startCall(shipment) } returns JobCallResponse(JobCallData(bridged = true, callSid = "CA1"))
        assertTrue(ChatViewModel(api).placeCall(shipment))
    }

    // Both of these mean the same thing to the screen: dial it yourself.
    @Test
    fun `a bridge that is off or has no number is not bridged`() = runTest {
        coEvery { api.startCall(shipment) } returns JobCallResponse(JobCallData(bridged = false, reason = "not_configured"))
        assertFalse(ChatViewModel(api).placeCall(shipment))

        coEvery { api.startCall(shipment) } returns JobCallResponse(JobCallData(bridged = false, reason = "no_number"))
        assertFalse(ChatViewModel(api).placeCall(shipment))
    }

    @Test
    fun `a failed request falls back rather than throwing at the screen`() = runTest {
        coEvery { api.startCall(shipment) } throws IOException("no signal")
        assertFalse(ChatViewModel(api).placeCall(shipment))
    }
}
