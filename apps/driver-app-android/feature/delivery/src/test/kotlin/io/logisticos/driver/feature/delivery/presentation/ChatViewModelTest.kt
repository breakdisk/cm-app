package io.logisticos.driver.feature.delivery.presentation

import io.logisticos.driver.core.network.service.EngagementApiService
import io.logisticos.driver.core.network.service.JobMessageItem
import io.logisticos.driver.core.network.service.JobMessageResponse
import io.logisticos.driver.core.network.service.JobThreadData
import io.logisticos.driver.core.network.service.JobThreadResponse
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
class ChatViewModelTest {
    private val api: EngagementApiService = mockk(relaxed = true)
    private val shipment = "ship-1"

    private fun message(id: String, minute: Int, role: String = "customer") = JobMessageItem(
        id = id,
        shipmentId = shipment,
        senderId = if (role == "driver") "me" else "customer-1",
        senderRole = role,
        body = "message $id",
        createdAt = "2026-09-16T09:%02d:00Z".format(minute),
    )

    @BeforeEach fun setUp() { Dispatchers.setMain(UnconfinedTestDispatcher()) }
    @AfterEach fun tearDown() { Dispatchers.resetMain() }

    @Test
    fun `the thread arrives oldest first`() = runTest {
        coEvery { api.listMessages(shipment) } returns JobThreadResponse(
            JobThreadData(messages = listOf(message("b", 3), message("a", 1)))
        )
        val vm = ChatViewModel(api)
        vm.refresh(shipment)

        assertEquals(listOf("a", "b"), vm.uiState.value.messages.map { it.id })
        assertFalse(vm.uiState.value.loading)
        assertNull(vm.uiState.value.error)
    }

    // The poll returns what this phone just sent; it must not show up twice.
    @Test
    fun `a message this phone sent is not duplicated by the next poll`() = runTest {
        val mine = message("m1", 4, role = "driver")
        coEvery { api.sendMessage(shipment, any()) } returns JobMessageResponse(mine)
        coEvery { api.listMessages(shipment) } returns JobThreadResponse(JobThreadData(messages = listOf(mine, message("a", 1))))

        val vm = ChatViewModel(api)
        vm.send(shipment, "On my way")
        vm.refresh(shipment)

        assertEquals(listOf("a", "m1"), vm.uiState.value.messages.map { it.id })
    }

    @Test
    fun `a finished job closes the composer`() = runTest {
        coEvery { api.listMessages(shipment) } returns JobThreadResponse(
            JobThreadData(canSend = false, shipmentStatus = "delivered", messages = listOf(message("a", 1)))
        )
        val vm = ChatViewModel(api)
        vm.refresh(shipment)

        assertFalse(vm.uiState.value.canSend)
    }

    @Test
    fun `someone else's job says so, rather than looking broken`() = runTest {
        coEvery { api.listMessages(shipment) } throws IOException("HTTP 403 Forbidden")
        val vm = ChatViewModel(api)
        vm.refresh(shipment)

        assertEquals("This thread belongs to the driver on this job.", vm.uiState.value.error)
    }

    @Test
    fun `an empty message is never sent`() = runTest {
        val vm = ChatViewModel(api)
        vm.send(shipment, "   ")
        assertTrue(vm.uiState.value.messages.isEmpty())
        assertFalse(vm.uiState.value.sending)
    }

    @Test
    fun `a send failure keeps the thread and says what happened`() = runTest {
        coEvery { api.sendMessage(shipment, any()) } throws IOException("no signal")
        val vm = ChatViewModel(api)
        vm.send(shipment, "Outside now")

        assertFalse(vm.uiState.value.sending)
        assertEquals("Messages aren't loading. They'll appear when you're back on signal.", vm.uiState.value.error)
    }
}
