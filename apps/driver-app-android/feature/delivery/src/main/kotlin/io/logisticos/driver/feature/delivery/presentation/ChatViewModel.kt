package io.logisticos.driver.feature.delivery.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import io.logisticos.driver.core.network.service.EngagementApiService
import io.logisticos.driver.core.network.service.JobMessageItem
import io.logisticos.driver.core.network.service.SendJobMessageRequest
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import java.util.UUID
import javax.inject.Inject

/** How often an open thread asks for anything new. */
private const val POLL_MS = 4_000L

data class ChatUiState(
    val loading: Boolean = true,
    val messages: List<JobMessageItem> = emptyList(),
    /** False once the job is over: the thread stays readable, and closed. */
    val canSend: Boolean = true,
    val sending: Boolean = false,
    val error: String? = null,
)

/**
 * The thread with the customer on one job.
 *
 * Nothing polls from `init` — the screen starts it when it appears and stops it
 * when it leaves, so a ViewModel built in a test never spins a timer.
 */
@HiltViewModel
class ChatViewModel @Inject constructor(
    private val api: EngagementApiService,
) : ViewModel() {

    private val _uiState = MutableStateFlow(ChatUiState())
    val uiState: StateFlow<ChatUiState> = _uiState.asStateFlow()

    private var poller: Job? = null

    fun startPolling(shipmentId: String) {
        if (poller?.isActive == true) return
        poller = viewModelScope.launch {
            while (isActive) {
                refresh(shipmentId)
                delay(POLL_MS)
            }
        }
    }

    fun stopPolling() {
        poller?.cancel()
        poller = null
    }

    suspend fun refresh(shipmentId: String) {
        runCatching { api.listMessages(shipmentId) }
            .onSuccess { response ->
                _uiState.update {
                    it.copy(
                        loading = false,
                        messages = mergeMessages(it.messages, response.data.messages),
                        canSend = response.data.canSend,
                        error = null,
                    )
                }
                // Reading it is reading it; the badge clears here.
                runCatching { api.markRead(shipmentId) }
            }
            .onFailure { e ->
                _uiState.update { it.copy(loading = false, error = chatError(e)) }
            }
    }

    fun send(shipmentId: String, body: String) {
        val text = body.trim()
        if (text.isEmpty() || _uiState.value.sending) return
        viewModelScope.launch {
            _uiState.update { it.copy(sending = true) }
            runCatching { api.sendMessage(shipmentId, SendJobMessageRequest(text, UUID.randomUUID().toString())) }
                .onSuccess { response ->
                    _uiState.update {
                        it.copy(sending = false, messages = mergeMessages(it.messages, listOf(response.data)), error = null)
                    }
                }
                .onFailure { e ->
                    _uiState.update { it.copy(sending = false, error = chatError(e)) }
                }
        }
    }

    override fun onCleared() {
        stopPolling()
        super.onCleared()
    }
}

/** What the driver is told. A 403 is a fact about this job, not a fault. */
internal fun chatError(e: Throwable): String {
    val text = e.message.orEmpty()
    return when {
        text.contains("403") -> "This thread belongs to the driver on this job."
        text.contains("404") -> "This job has no thread yet."
        else -> "Messages aren't loading. They'll appear when you're back on signal."
    }
}
