package io.logisticos.driver.feature.assignment.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import dagger.hilt.android.lifecycle.HiltViewModel
import io.logisticos.driver.core.common.AssignmentPayload
import io.logisticos.driver.core.common.TaskSyncBus
import io.logisticos.driver.feature.assignment.data.AssignmentRepository
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

data class AssignmentUiState(
    val assignmentId:    String  = "",
    val shipmentId:      String  = "",
    val customerName:    String  = "",
    val address:         String  = "",
    val taskType:        String  = "delivery",  // "pickup" | "delivery"
    val trackingNumber:  String  = "",
    val codAmountCents:  Long    = 0L,
    val isAccepting:     Boolean = false,
    val isRejecting:     Boolean = false,
    val showRejectSheet: Boolean = false,
    val error:           String? = null,
    /** True once accept or reject succeeds — screen calls onDone(). */
    val isDone:          Boolean = false,
)

@HiltViewModel(assistedFactory = AssignmentViewModel.Factory::class)
class AssignmentViewModel @AssistedInject constructor(
    private val repo: AssignmentRepository,
    @Assisted private val payload: AssignmentPayload,
) : ViewModel() {

    @AssistedFactory
    interface Factory {
        fun create(payload: AssignmentPayload): AssignmentViewModel
    }

    private val _uiState = MutableStateFlow(
        AssignmentUiState(
            assignmentId   = payload.assignmentId,
            shipmentId     = payload.shipmentId,
            customerName   = payload.customerName,
            address        = payload.address,
            taskType       = payload.taskType,
            trackingNumber = payload.trackingNumber,
            codAmountCents = payload.codAmountCents,
        )
    )
    val uiState: StateFlow<AssignmentUiState> = _uiState.asStateFlow()

    /** Driver taps "Accept". Calls backend, triggers task sync, emits isDone. */
    fun accept() {
        viewModelScope.launch {
            _uiState.update { it.copy(isAccepting = true, error = null) }
            repo.accept(payload.assignmentId)
                .onSuccess {
                    TaskSyncBus.requestSync()
                    _uiState.update { it.copy(isAccepting = false, isDone = true) }
                }
                .onFailure { e ->
                    _uiState.update { it.copy(isAccepting = false, error = e.message) }
                }
        }
    }

    /** Driver taps "Reject" and selects a reason. */
    fun reject(reason: String) {
        viewModelScope.launch {
            _uiState.update { it.copy(isRejecting = true, showRejectSheet = false, error = null) }
            repo.reject(payload.assignmentId, reason)
                .onSuccess {
                    _uiState.update { it.copy(isRejecting = false, isDone = true) }
                }
                .onFailure { e ->
                    _uiState.update { it.copy(isRejecting = false, error = e.message) }
                }
        }
    }

    fun showRejectSheet()    { _uiState.update { it.copy(showRejectSheet = true) } }
    fun dismissRejectSheet() { _uiState.update { it.copy(showRejectSheet = false) } }
    fun clearError()         { _uiState.update { it.copy(error = null) } }
}
