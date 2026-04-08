package io.logisticos.driver.feature.delivery.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import io.logisticos.driver.core.database.entity.TaskStatus
import io.logisticos.driver.feature.delivery.data.DeliveryRepository
import kotlinx.coroutines.launch
import javax.inject.Inject

@HiltViewModel
class ArrivalViewModel @Inject constructor(
    private val repo: DeliveryRepository
) : ViewModel() {

    /**
     * Transitions the task from ARRIVED → IN_PROGRESS, then invokes [onReady].
     * The state machine requires IN_PROGRESS before COMPLETED can be set by the POD screen.
     */
    fun startDelivery(taskId: String, onReady: () -> Unit) {
        viewModelScope.launch {
            repo.transitionTask(taskId, TaskStatus.IN_PROGRESS)
            onReady()
        }
    }
}
