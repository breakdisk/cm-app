package io.logisticos.driver.feature.route.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import dagger.hilt.android.lifecycle.HiltViewModel
import io.logisticos.driver.core.database.entity.TaskEntity
import io.logisticos.driver.core.database.entity.TaskStatus
import io.logisticos.driver.feature.route.data.RouteRepository
import kotlinx.coroutines.flow.*
import kotlinx.coroutines.launch

data class RouteUiState(
    val activeTasks: List<TaskEntity> = emptyList(),
    val completedTasks: List<TaskEntity> = emptyList(),
    val isLoading: Boolean = false
)

@HiltViewModel(assistedFactory = RouteViewModel.Factory::class)
class RouteViewModel @AssistedInject constructor(
    private val repo: RouteRepository,
    @Assisted private val shiftId: String
) : ViewModel() {

    @AssistedFactory
    interface Factory {
        fun create(shiftId: String): RouteViewModel
    }

    private val _uiState = MutableStateFlow(RouteUiState())
    val uiState: StateFlow<RouteUiState> = _uiState.asStateFlow()

    private var reorderedActive = mutableListOf<TaskEntity>()

    init {
        viewModelScope.launch {
            repo.observeTasks(shiftId).collect { tasks ->
                val active = tasks.filter {
                    it.status !in listOf(
                        TaskStatus.COMPLETED,
                        TaskStatus.RETURNED,
                        TaskStatus.FAILED
                    )
                }
                val completed = tasks.filter { it.status == TaskStatus.COMPLETED }
                reorderedActive = active.toMutableList()
                _uiState.update { it.copy(activeTasks = active, completedTasks = completed) }
            }
        }
    }

    fun reorder(fromIndex: Int, toIndex: Int) {
        val list = reorderedActive.toMutableList()
        if (fromIndex !in list.indices || toIndex !in list.indices) return
        val item = list.removeAt(fromIndex)
        list.add(toIndex, item)
        reorderedActive = list
        _uiState.update { it.copy(activeTasks = list) }
        viewModelScope.launch {
            list.forEachIndexed { index, task ->
                repo.updateStopOrder(task.id, index + 1)
            }
        }
    }
}
