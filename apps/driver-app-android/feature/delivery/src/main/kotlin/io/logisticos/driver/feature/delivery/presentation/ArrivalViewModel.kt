package io.logisticos.driver.feature.delivery.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import io.logisticos.driver.core.database.entity.TaskEntity
import io.logisticos.driver.core.database.entity.TaskStatus
import io.logisticos.driver.feature.delivery.data.DeliveryRepository
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import javax.inject.Inject

data class ArrivalUiState(
    val task: TaskEntity? = null,
    val isTransitioning: Boolean = false,
    val error: String? = null
)

@HiltViewModel
class ArrivalViewModel @Inject constructor(
    private val repo: DeliveryRepository
) : ViewModel() {

    private val _uiState = MutableStateFlow(ArrivalUiState())
    val uiState: StateFlow<ArrivalUiState> = _uiState.asStateFlow()

    fun load(taskId: String) {
        viewModelScope.launch {
            repo.observeTask(taskId)
                .filterNotNull()
                .first()
                .let { task -> _uiState.update { it.copy(task = task) } }
        }
    }

    /**
     * Transitions ARRIVED → IN_PROGRESS then invokes [onReady].
     * State machine enforces IN_PROGRESS before COMPLETED can be set by POD screen.
     */
    fun startTask(taskId: String, onReady: () -> Unit) {
        viewModelScope.launch {
            _uiState.update { it.copy(isTransitioning = true) }
            runCatching {
                repo.transitionTask(taskId, TaskStatus.IN_PROGRESS)
            }.onSuccess {
                _uiState.update { it.copy(isTransitioning = false) }
                onReady()
            }.onFailure { e ->
                _uiState.update { it.copy(isTransitioning = false, error = e.message) }
            }
        }
    }
}
