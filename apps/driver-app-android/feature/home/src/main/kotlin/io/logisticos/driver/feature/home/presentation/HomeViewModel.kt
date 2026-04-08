package io.logisticos.driver.feature.home.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import io.logisticos.driver.core.database.entity.ShiftEntity
import io.logisticos.driver.feature.home.data.ShiftRepository
import kotlinx.coroutines.flow.*
import kotlinx.coroutines.launch
import javax.inject.Inject

data class HomeUiState(
    val shift: ShiftEntity? = null,
    val isLoading: Boolean = false,
    val error: String? = null,
    val isOfflineMode: Boolean = false
)

@HiltViewModel
class HomeViewModel @Inject constructor(
    private val repo: ShiftRepository
) : ViewModel() {

    private val _uiState = MutableStateFlow(HomeUiState())
    val uiState: StateFlow<HomeUiState> = _uiState.asStateFlow()

    init {
        viewModelScope.launch {
            repo.observeActiveShift().collect { shift ->
                _uiState.update { it.copy(shift = shift) }
            }
        }
        syncShift()
    }

    fun syncShift() {
        viewModelScope.launch {
            _uiState.update { it.copy(isLoading = true) }
            runCatching { repo.syncShift() }
                .onFailure { e -> _uiState.update { it.copy(error = e.message, isOfflineMode = true) } }
            _uiState.update { it.copy(isLoading = false) }
        }
    }
}
