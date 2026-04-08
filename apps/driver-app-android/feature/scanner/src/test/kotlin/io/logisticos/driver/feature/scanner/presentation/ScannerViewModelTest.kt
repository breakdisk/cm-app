package io.logisticos.driver.feature.scanner.presentation

import app.cash.turbine.test
import io.logisticos.driver.feature.scanner.domain.ScanResult
import io.logisticos.driver.feature.scanner.domain.ScanValidationResult
import io.logisticos.driver.feature.scanner.domain.ScannerManager
import io.mockk.mockk
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.jupiter.api.AfterEach
import org.junit.jupiter.api.BeforeEach
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.Assertions.*

@OptIn(ExperimentalCoroutinesApi::class)
class ScannerViewModelTest {
    private val testDispatcher = UnconfinedTestDispatcher()
    private val scannerManager: ScannerManager = mockk(relaxed = true)
    private lateinit var vm: ScannerViewModel

    @BeforeEach fun setUp() {
        Dispatchers.setMain(testDispatcher)
        vm = ScannerViewModel(scannerManager)
        vm.setExpectedAwbs(listOf("LS-ABC123", "LS-DEF456"))
    }

    @AfterEach fun tearDown() { Dispatchers.resetMain() }

    @Test
    fun `scanning expected AWB adds to scanned list`() = runTest {
        vm.uiState.test {
            awaitItem()
            vm.onScanResult(ScanResult("LS-ABC123", "QR_CODE"))
            val state = awaitItem()
            assertTrue(state.scannedAwbs.contains("LS-ABC123"))
            assertTrue(state.lastValidation is ScanValidationResult.Match)
        }
    }

    @Test
    fun `scanning unexpected AWB sets Unexpected validation`() = runTest {
        vm.uiState.test {
            awaitItem()
            vm.onScanResult(ScanResult("LS-UNKNOWN", "QR_CODE"))
            val state = awaitItem()
            assertTrue(state.lastValidation is ScanValidationResult.Unexpected)
        }
    }

    @Test
    fun `allScanned is true when all expected AWBs are scanned`() = runTest {
        vm.uiState.test {
            awaitItem()
            vm.onScanResult(ScanResult("LS-ABC123", "QR_CODE"))
            awaitItem()
            vm.onScanResult(ScanResult("LS-DEF456", "QR_CODE"))
            val state = awaitItem()
            assertTrue(state.allScanned)
        }
    }
}
