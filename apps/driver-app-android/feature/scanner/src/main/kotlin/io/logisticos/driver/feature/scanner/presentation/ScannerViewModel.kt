package io.logisticos.driver.feature.scanner.presentation

import androidx.lifecycle.ViewModel
import dagger.hilt.android.lifecycle.HiltViewModel
import io.logisticos.driver.feature.scanner.domain.ScanResult
import io.logisticos.driver.feature.scanner.domain.ScanValidationResult
import io.logisticos.driver.feature.scanner.domain.ScanValidator
import io.logisticos.driver.feature.scanner.domain.ScannerManager
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import javax.inject.Inject

data class ScannerUiState(
    val expectedAwbs: List<String> = emptyList(),
    val scannedAwbs: List<String> = emptyList(),
    val lastValidation: ScanValidationResult? = null,
    val allScanned: Boolean = false,
    val hasUnresolvedWarnings: Boolean = false
)

@HiltViewModel
class ScannerViewModel @Inject constructor(
    private val scannerManager: ScannerManager
) : ViewModel() {

    private val _uiState = MutableStateFlow(ScannerUiState())
    val uiState: StateFlow<ScannerUiState> = _uiState.asStateFlow()

    fun setExpectedAwbs(awbs: List<String>) {
        _uiState.update { it.copy(expectedAwbs = awbs) }
    }

    fun onScanResult(result: ScanResult) {
        val state = _uiState.value
        val validation = ScanValidator.validate(
            scannedAwb = result.rawValue,
            expectedAwbs = state.expectedAwbs,
            alreadyScanned = state.scannedAwbs
        )
        val newScanned = if (validation is ScanValidationResult.Match) {
            state.scannedAwbs + result.rawValue
        } else state.scannedAwbs
        _uiState.update {
            it.copy(
                scannedAwbs = newScanned,
                lastValidation = validation,
                allScanned = state.expectedAwbs.isNotEmpty() && newScanned.containsAll(state.expectedAwbs),
                hasUnresolvedWarnings = validation is ScanValidationResult.Unexpected
            )
        }
    }

    fun acknowledgeUnexpected() {
        _uiState.update { it.copy(hasUnresolvedWarnings = false, lastValidation = null) }
    }

    override fun onCleared() {
        scannerManager.stopScan()
        super.onCleared()
    }
}
