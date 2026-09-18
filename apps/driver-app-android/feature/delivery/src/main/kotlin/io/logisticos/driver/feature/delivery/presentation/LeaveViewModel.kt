package io.logisticos.driver.feature.delivery.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import io.logisticos.driver.core.network.service.DriverOpsApiService
import io.logisticos.driver.core.network.service.LeaveQuoteData
import io.logisticos.driver.core.network.service.LeaveTaskRequest
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import retrofit2.HttpException
import javax.inject.Inject

/** A reason chip. The code goes to the server; the label is copy. */
data class LeaveReason(val code: String, val label: String)

/** Leaving a job the driver accepted: their side of it. */
val DROP_REASONS = listOf(
    LeaveReason("VEHICLE_TROUBLE", "Vehicle trouble"),
    LeaveReason("CANNOT_MAKE_WINDOW", "Can't make the window"),
    LeaveReason("LOAD_NOT_AS_DESCRIBED", "Load not as described"),
    LeaveReason("PERSONAL_EMERGENCY", "Personal emergency"),
    LeaveReason("OTHER", "Something else"),
)

/** Leaving after grace: the customer's side of it. */
val RELEASE_REASONS = listOf(
    LeaveReason("CUSTOMER_NOT_ANSWERING", "Customer not answering"),
    LeaveReason("NOBODY_AT_ADDRESS", "Nobody at the address"),
    LeaveReason("ACCESS_REFUSED", "Couldn't get access"),
    LeaveReason("OTHER", "Something else"),
)

fun reasonsFor(mode: String?): List<LeaveReason> = if (mode == "release") RELEASE_REASONS else DROP_REASONS

/** Plain words for the server's refusals. */
fun refusalMessage(code: String?): String? = when (code) {
    "GOODS_ABOARD" ->
        "The load is aboard and the customer isn't late yet. If you can't deliver, report a failed delivery instead."
    "TASK_CLOSED" -> "This stop is already closed."
    null -> null
    else -> "You can't leave this stop from here."
}

data class LeaveUiState(
    val loading: Boolean = true,
    val quote: LeaveQuoteData? = null,
    val reason: String? = null,
    val note: String = "",
    val submitting: Boolean = false,
    /** The quote as the server applied it. Set = done; the screen leaves. */
    val applied: LeaveQuoteData? = null,
    val error: String? = null,
)

/**
 * Leaving an accepted job. The server decides whether it is a drop or a
 * release; this shows what it said, takes a reason, and asks it to apply.
 */
@HiltViewModel
class LeaveViewModel @Inject constructor(
    private val api: DriverOpsApiService,
) : ViewModel() {

    private val _uiState = MutableStateFlow(LeaveUiState())
    val uiState: StateFlow<LeaveUiState> = _uiState.asStateFlow()

    fun load(taskId: String) {
        viewModelScope.launch { refresh(taskId) }
    }

    suspend fun refresh(taskId: String) {
        _uiState.update { it.copy(loading = it.quote == null) }
        runCatching { api.getLeaveQuote(taskId).data }
            .onSuccess { quote ->
                _uiState.update { s ->
                    // A mode that flipped (grace ran out while they read) takes
                    // a reason from its own list.
                    val keep = s.reason?.takeIf { code -> reasonsFor(quote.mode).any { it.code == code } }
                    s.copy(loading = false, quote = quote, reason = keep, error = null)
                }
            }
            .onFailure { e ->
                _uiState.update { it.copy(loading = false, error = e.message ?: "Couldn't reach the server.") }
            }
    }

    fun pickReason(code: String) = _uiState.update { it.copy(reason = code, error = null) }

    fun setNote(text: String) = _uiState.update { it.copy(note = text.take(500)) }

    fun confirm(taskId: String, lat: Double?, lng: Double?) {
        val state = _uiState.value
        val reason = state.reason
        if (reason == null) {
            _uiState.update { it.copy(error = "Pick a reason first.") }
            return
        }
        if (state.submitting) return
        viewModelScope.launch {
            _uiState.update { it.copy(submitting = true, error = null) }
            runCatching {
                api.leaveTask(
                    taskId,
                    LeaveTaskRequest(reasonCode = reason, note = state.note.trim().ifBlank { null }, lat = lat, lng = lng),
                ).data
            }
                .onSuccess { applied -> _uiState.update { it.copy(submitting = false, applied = applied) } }
                .onFailure { e ->
                    val body = (e as? HttpException)?.response()?.errorBody()?.string().orEmpty()
                    val code = listOf("GOODS_ABOARD", "TASK_CLOSED").firstOrNull { body.contains(it) }
                    // Whatever changed server-side, show where things stand now,
                    // then say why — a refresh clears the error it finds.
                    refresh(taskId)
                    _uiState.update {
                        it.copy(
                            submitting = false,
                            error = refusalMessage(code) ?: "Couldn't leave the job. Try again.",
                        )
                    }
                }
        }
    }
}
