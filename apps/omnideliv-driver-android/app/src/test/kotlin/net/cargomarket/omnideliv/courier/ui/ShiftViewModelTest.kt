package net.cargomarket.omnideliv.courier.ui

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import net.cargomarket.omnideliv.courier.data.ClaimDto
import net.cargomarket.omnideliv.courier.data.CourierApi
import net.cargomarket.omnideliv.courier.data.MyOffersDto
import net.cargomarket.omnideliv.courier.data.OfferDto
import net.cargomarket.omnideliv.courier.data.SetStatusRequest
import okhttp3.ResponseBody.Companion.toResponseBody
import org.junit.jupiter.api.AfterEach
import org.junit.jupiter.api.Assertions.assertInstanceOf
import org.junit.jupiter.api.Assertions.assertNotNull
import org.junit.jupiter.api.BeforeEach
import org.junit.jupiter.api.Test
import retrofit2.Response

/**
 * Going on duty is a claim about the *server's* state, not the phone's.
 *
 * The app shipped with a toggle that changed a local flag and nothing else: the
 * courier read "On duty - Watching for offers" while field-ops still had them
 * `offline`, so the proximity search skipped them and no order could ever
 * arrive. A toggle that lies about this is worse than no toggle, because the
 * courier waits on it.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ShiftViewModelTest {

    private val api = mockk<CourierApi>()
    private val dispatcher = StandardTestDispatcher()

    // Two rules, both learned the same way — the worker dies of
    // OutOfMemoryError rather than hanging or failing, and the report names no
    // test at all:
    //
    // 1. `runCurrent`, never `advanceUntilIdle`. Going on duty starts a poller
    //    that loops forever on `delay(POLL_MS)`; advancing until idle against
    //    an unbounded loop advances virtual time forever, allocating one
    //    recorded mock call per iteration.
    // 2. Every test that leaves the courier on duty must end by taking them off
    //    it. `runTest` drains the scheduler after the body returns, and it
    //    meets the same unbounded loop there.

    @BeforeEach fun setUp() = Dispatchers.setMain(dispatcher)

    @AfterEach fun tearDown() = Dispatchers.resetMain()

    private fun noOffers() = Response.success(MyOffersDto(offers = emptyList()))

    private fun offer() = Response.success(
        MyOffersDto(
            offers = listOf(
                OfferDto(
                    assignmentId = "assignment-1",
                    product = "omnideliv",
                    externalRef = "order-1",
                    tripCents = 3_500L,
                    tipCents = 0L,
                    codAmountCents = 0L,
                    offerCard = null,
                    offeredAt = "2026-08-19T10:00:00Z",
                ),
            ),
        ),
    )

    @Test
    fun `going on duty tells the server before listening for offers`() = runTest {
        coEvery { api.setStatus(any()) } returns Response.success(Unit)
        coEvery { api.myOffers() } returns noOffers()
        val vm = ShiftViewModel(api)

        vm.goOnline()
        runCurrent()

        coVerify(exactly = 1) { api.setStatus(SetStatusRequest(available = true)) }
        assertInstanceOf(ShiftState.Online::class.java, vm.state.value)

        vm.goOffline() // Cancels the poller. See the note above; not optional.
    }

    /**
     * The failure that matters. If the server did not agree, the courier is not
     * on duty and must not be told they are — they would sit watching a list
     * that can never fill.
     */
    @Test
    fun `a courier is not shown as on duty when the server refuses`() = runTest {
        coEvery { api.setStatus(any()) } returns Response.error(500, "".toResponseBody())
        coEvery { api.myOffers() } returns noOffers()
        val vm = ShiftViewModel(api)

        vm.goOnline()
        runCurrent()

        val state = vm.state.value
        assertInstanceOf(ShiftState.Offline::class.java, state)
        assertNotNull((state as ShiftState.Offline).notice, "the courier must be told why")
        coVerify(exactly = 0) { api.myOffers() }
    }

    @Test
    fun `going off duty tells the server`() = runTest {
        coEvery { api.setStatus(any()) } returns Response.success(Unit)
        coEvery { api.myOffers() } returns noOffers()
        val vm = ShiftViewModel(api)
        vm.goOnline()
        runCurrent()

        vm.goOffline()
        runCurrent()

        coVerify(exactly = 1) { api.setStatus(SetStatusRequest(available = false)) }
        assertInstanceOf(ShiftState.Offline::class.java, vm.state.value)
    }

    /**
     * Claiming stops the offer poll — the courier has a job and is not shopping
     * for another — but it must not report them off duty. They are working, and
     * the supply query already excludes a courier holding a live claim by
     * asking the claim index directly. Saying it again here would leave them
     * offline the moment the job ended.
     */
    @Test
    fun `claiming a job does not report the courier off duty`() = runTest {
        coEvery { api.setStatus(any()) } returns Response.success(Unit)
        coEvery { api.myOffers() } returns offer()
        coEvery { api.claim(any()) } returns Response.success(ClaimDto(won = true))
        val vm = ShiftViewModel(api)
        vm.goOnline()
        runCurrent()

        vm.claim("assignment-1")
        runCurrent()

        assertInstanceOf(ShiftState.Claimed::class.java, vm.state.value)
        coVerify(exactly = 0) { api.setStatus(SetStatusRequest(available = false)) }
    }
}
