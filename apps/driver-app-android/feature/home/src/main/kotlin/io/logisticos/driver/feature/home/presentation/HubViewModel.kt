package io.logisticos.driver.feature.home.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import io.logisticos.driver.core.database.dao.ShiftDao
import io.logisticos.driver.core.database.dao.TaskDao
import io.logisticos.driver.core.database.entity.TaskEntity
import io.logisticos.driver.core.database.entity.TaskStatus
import io.logisticos.driver.core.database.entity.TaskType
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import javax.inject.Inject

/**
 * Hub-operations screen state: the subset of the driver's active shift
 * tasks that move parcels through a sortation hub — first-mile pickups
 * destined for the hub, returns being dropped off, and the dedicated
 * HUB_DROP task type.
 *
 * Showing all three on one screen means the driver hits one button when
 * they're at the hub rather than scrolling the full route looking for
 * the relevant stops.
 */
data class HubUiState(
    val hubTasks: List<TaskEntity> = emptyList(),
    val isLoading: Boolean = true,
)

@HiltViewModel
class HubViewModel @Inject constructor(
    private val shiftDao: ShiftDao,
    private val taskDao: TaskDao,
) : ViewModel() {

    private val _uiState = MutableStateFlow(HubUiState())
    val uiState: StateFlow<HubUiState> = _uiState.asStateFlow()

    init {
        observeHubTasks()
    }

    @OptIn(ExperimentalCoroutinesApi::class)
    private fun observeHubTasks() {
        viewModelScope.launch {
            shiftDao.getActiveShift()
                .flatMapLatest { shift ->
                    if (shift == null) flowOf(emptyList())
                    else taskDao.getTasksForShift(shift.id)
                }
                .collect { tasks ->
                    val hub = tasks
                        .filter { it.taskType == TaskType.HUB_DROP || it.taskType == TaskType.RETURN }
                        .filter { it.status != TaskStatus.COMPLETED && it.status != TaskStatus.FAILED }
                        .sortedBy { it.stopOrder }
                    _uiState.update { it.copy(hubTasks = hub, isLoading = false) }
                }
        }
    }
}
