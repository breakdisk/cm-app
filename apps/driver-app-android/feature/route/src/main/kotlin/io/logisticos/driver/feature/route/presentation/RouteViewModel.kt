package io.logisticos.driver.feature.route.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import dagger.hilt.android.lifecycle.HiltViewModel
import io.logisticos.driver.core.database.dao.ShiftDao
import io.logisticos.driver.core.database.entity.TaskEntity
import io.logisticos.driver.core.database.entity.TaskStatus
import io.logisticos.driver.feature.route.data.RouteRepository
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.*
import kotlinx.coroutines.launch
import java.util.concurrent.atomic.AtomicBoolean

data class RouteUiState(
    val activeTasks: List<TaskEntity> = emptyList(),
    val completedTasks: List<TaskEntity> = emptyList(),
    val isLoading: Boolean = false
)

@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel(assistedFactory = RouteViewModel.Factory::class)
class RouteViewModel @AssistedInject constructor(
    private val repo: RouteRepository,
    private val shiftDao: ShiftDao,
    @Assisted private val shiftId: String
) : ViewModel() {

    @AssistedFactory
    interface Factory {
        fun create(shiftId: String): RouteViewModel
    }

    private val _uiState = MutableStateFlow(RouteUiState())
    val uiState: StateFlow<RouteUiState> = _uiState.asStateFlow()

    private val isReordering = AtomicBoolean(false)
    private var reorderedActive = mutableListOf<TaskEntity>()

    init {
        viewModelScope.launch {
            // Resolve shift_id reactively so a sync that completes *after*
            // RouteScreen mounts (race when the user lands on Route before
            // HomeViewModel.syncShift inserts the synthetic shift) still wires
            // the task observer correctly. Previously this called
            // getActiveShiftOnce() which returned a snapshot — null at
            // construction → observed shift_id="" forever, so tasks inserted
            // a moment later never reached the screen.
            val shiftIdFlow = if (shiftId.isEmpty()) {
                shiftDao.getActiveShift().filterNotNull().map { it.id }
            } else {
                flowOf(shiftId)
            }
            shiftIdFlow.flatMapLatest { id ->
                repo.observeTasks(id)
            }.collect { tasks ->
                val active = tasks.filter {
                    it.status !in listOf(
                        TaskStatus.COMPLETED,
                        TaskStatus.RETURNED,
                        TaskStatus.FAILED
                    )
                }
                val completed = tasks.filter { it.status == TaskStatus.COMPLETED }
                if (!isReordering.get()) {
                    reorderedActive = active.toMutableList()
                }
                _uiState.update { state ->
                    if (!isReordering.get()) {
                        state.copy(activeTasks = active, completedTasks = completed)
                    } else {
                        state.copy(completedTasks = completed)
                    }
                }
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
        isReordering.set(true)
        viewModelScope.launch {
            list.forEachIndexed { index, task ->
                repo.updateStopOrder(task.id, index + 1)
            }
            isReordering.set(false)
        }
    }
}
