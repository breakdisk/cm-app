package io.logisticos.driver.feature.home.presentation

import android.content.Context
import app.cash.turbine.test
import io.logisticos.driver.core.database.dao.SyncQueueDao
import io.logisticos.driver.core.database.worker.SyncRecovery
import io.logisticos.driver.core.database.entity.ShiftEntity
import io.logisticos.driver.core.location.LocationRepository
import io.logisticos.driver.core.network.auth.SessionManager
import io.logisticos.driver.core.network.service.ComplianceApiService
import io.logisticos.driver.core.network.service.DriverOpsApiService
import io.logisticos.driver.core.network.service.IdentityApiService
import io.logisticos.driver.feature.home.data.ShiftRepository
import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.test.*
import org.junit.jupiter.api.*
import org.junit.jupiter.api.Assertions.*
import java.io.IOException

@OptIn(ExperimentalCoroutinesApi::class)
class HomeViewModelTest {
    private val repo:          ShiftRepository      = mockk()
    private val api:           DriverOpsApiService  = mockk(relaxed = true)
    private val complianceApi: ComplianceApiService = mockk(relaxed = true)
    private val identityApi:   IdentityApiService   = mockk(relaxed = true)
    private val locationRepo:  LocationRepository   = mockk(relaxed = true)
    private val syncQueueDao:  SyncQueueDao         = mockk(relaxed = true)
    private val syncRecovery:  SyncRecovery         = mockk(relaxed = true)
    private val sessionManager: SessionManager      = mockk(relaxed = true)
    private val context:       Context              = mockk(relaxed = true)

    private lateinit var vm: HomeViewModel

    @BeforeEach fun setUp() {
        // Deliberately a real dispatcher, not UnconfinedTestDispatcher — unlike every
        // other ViewModel test in this app.
        //
        // HomeViewModel.init starts four unbounded `while (true) { delay(...) }`
        // pollers on viewModelScope (the TTL ticker, startOfferPolling, startPolling
        // and the go-online heartbeat). It is the only ViewModel here that does.
        // Those run on Dispatchers.Main, and when Main is a TestDispatcher, runTest
        // adopts its TestCoroutineScheduler so the two share one virtual clock.
        // Virtual time costs nothing to advance, so runTest's end-of-test
        // "advance until idle" can never reach idle: each loop schedules its next
        // delay the instant the previous one fires. The suite then spins at full
        // CPU, and because every collaborator here is a relaxed mock — and MockK
        // records every call it receives — the recorded-call log grows without
        // bound until the test JVM dies of OutOfMemoryError.
        //
        // On CI that OOM landed in the Gradle worker's connection thread, which
        // killed the channel carrying the task result. The build did not fail; it
        // hung until GitHub's 6-hour job limit, twice.
        //
        // Handing Main a non-test dispatcher decouples the two clocks: runTest gets
        // its own scheduler and goes idle immediately, while the pollers park on
        // real timers and never tick during a sub-second test. Unconfined keeps
        // init eager, so uiState is populated by the time the constructor returns,
        // which is what these tests assert on.
        //
        // Consequence: virtual-time control (advanceTimeBy/advanceUntilIdle) is not
        // available for this ViewModel. It never was — the infinite pollers make
        // that impossible regardless. Testing the poll intervals themselves needs
        // the polling hoisted out of init behind an injected dispatcher first.
        Dispatchers.setMain(Dispatchers.Unconfined)
        val shift = ShiftEntity("s1", "d1", "t1", null, null, true, 5, 2, 0, 0.0, null)
        every { repo.observeActiveShift() } returns flowOf(shift)
        coEvery { repo.syncShift() } returns Unit
        every { syncQueueDao.getPendingCount() } returns flowOf(0)
        every { sessionManager.isHubScanner() } returns false
        every { sessionManager.getHubId() } returns null
        // Must be a real SharedFlow, not the relaxed default. HomeViewModel's init
        // collects this, and SharedFlow.collect is declared to return `Nothing`;
        // a relaxed mock returns normally, so the compiler-inserted check fires
        // KotlinNothingValueException inside init. That escapes as an uncaught
        // coroutine exception and every test then dies with
        // UncaughtExceptionsBeforeTest, which names neither the cause nor the site.
        // An empty MutableSharedFlow suspends forever, which is what the real one does.
        every { locationRepo.locationUpdates } returns MutableSharedFlow()
        // Availability-mode default: no active shift unless a test says otherwise.
        coEvery { repo.getActiveShiftId() } returns null
        vm = HomeViewModel(
            context      = context,
            repo         = repo,
            api          = api,
            complianceApi = complianceApi,
            identityApi  = identityApi,
            locationRepo = locationRepo,
            syncQueueDao = syncQueueDao,
            syncRecovery = syncRecovery,
            sessionManager = sessionManager,
        )
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

    @Test
    fun `sync failure sets offline mode or error`() = runTest {
        coEvery { repo.syncShift() } throws RuntimeException("Network error")
        val failVm = HomeViewModel(
            context      = context,
            repo         = repo,
            api          = api,
            complianceApi = complianceApi,
            identityApi  = identityApi,
            locationRepo = locationRepo,
            syncQueueDao = syncQueueDao,
            syncRecovery = syncRecovery,
            sessionManager = sessionManager,
        )
        failVm.uiState.test {
            val finalState = awaitItem()
            assertTrue(finalState.isOfflineMode || finalState.error != null || !finalState.isLoading)
        }
    }

    @Test
    fun `isHubScanner flag is loaded from session manager`() = runTest {
        every { sessionManager.isHubScanner() } returns true
        every { sessionManager.getHubId() } returns "hub-42"
        // init renders in two phases: the cached role from SessionManager first,
        // then refreshHubProfile() overwrites it from identity/driver-ops so a
        // mid-shift role change lands without re-login. The server is authoritative
        // when it answers, so this test — which is about the cached phase — has to
        // stop it answering. Left relaxed, both calls return a default-constructed
        // success (empty roles, null hub_id) that reads as "server says you are not
        // a hub scanner" and clobbers the cache, which is correct behaviour and
        // exactly what failed here once the suite actually started running.
        coEvery { identityApi.getMe() } throws IOException("offline")
        coEvery { api.getMyProfile() } throws IOException("offline")
        val hubVm = HomeViewModel(
            context      = context,
            repo         = repo,
            api          = api,
            complianceApi = complianceApi,
            identityApi  = identityApi,
            locationRepo = locationRepo,
            syncQueueDao = syncQueueDao,
            syncRecovery = syncRecovery,
            sessionManager = sessionManager,
        )
        hubVm.uiState.test {
            val state = awaitItem()
            assertTrue(state.isHubScanner)
            assertEquals("hub-42", state.hubId)
        }
    }
}
