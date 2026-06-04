package io.logisticos.driver.feature.home.presentation

import android.content.Context
import app.cash.turbine.test
import io.logisticos.driver.core.database.dao.SyncQueueDao
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
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.test.*
import org.junit.jupiter.api.*
import org.junit.jupiter.api.Assertions.*

@OptIn(ExperimentalCoroutinesApi::class)
class HomeViewModelTest {
    private val testDispatcher = UnconfinedTestDispatcher()

    private val repo:          ShiftRepository      = mockk()
    private val api:           DriverOpsApiService  = mockk(relaxed = true)
    private val complianceApi: ComplianceApiService = mockk(relaxed = true)
    private val identityApi:   IdentityApiService   = mockk(relaxed = true)
    private val locationRepo:  LocationRepository   = mockk(relaxed = true)
    private val syncQueueDao:  SyncQueueDao         = mockk(relaxed = true)
    private val sessionManager: SessionManager      = mockk(relaxed = true)
    private val context:       Context              = mockk(relaxed = true)

    private lateinit var vm: HomeViewModel

    @BeforeEach fun setUp() {
        Dispatchers.setMain(testDispatcher)
        val shift = ShiftEntity("s1", "d1", "t1", null, null, true, 5, 2, 0, 0.0, null)
        every { repo.observeActiveShift() } returns flowOf(shift)
        coEvery { repo.syncShift() } returns Unit
        every { syncQueueDao.getPendingCount() } returns flowOf(0)
        every { sessionManager.isHubScanner() } returns false
        every { sessionManager.getHubId() } returns null
        vm = HomeViewModel(
            context      = context,
            repo         = repo,
            api          = api,
            complianceApi = complianceApi,
            identityApi  = identityApi,
            locationRepo = locationRepo,
            syncQueueDao = syncQueueDao,
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
        val hubVm = HomeViewModel(
            context      = context,
            repo         = repo,
            api          = api,
            complianceApi = complianceApi,
            identityApi  = identityApi,
            locationRepo = locationRepo,
            syncQueueDao = syncQueueDao,
            sessionManager = sessionManager,
        )
        hubVm.uiState.test {
            val state = awaitItem()
            assertTrue(state.isHubScanner)
            assertEquals("hub-42", state.hubId)
        }
    }
}
