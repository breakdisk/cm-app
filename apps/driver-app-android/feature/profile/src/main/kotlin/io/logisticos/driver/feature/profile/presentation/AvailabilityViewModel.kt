package io.logisticos.driver.feature.profile.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import io.logisticos.driver.core.network.service.AvailabilityRequest
import io.logisticos.driver.core.network.service.HomeMoveApiService
import io.logisticos.driver.core.network.service.ProviderProfileDto
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import retrofit2.HttpException
import java.time.LocalDate
import javax.inject.Inject

data class AvailabilityUiState(
    val loading: Boolean = true,
    val error: String? = null,
    /** Operations' onboarding; shown, not edited here. */
    val profile: ProviderProfileDto? = null,
    val workingDays: List<Int> = emptyList(),
    /** YYYY-MM-DD, in order. */
    val offDays: List<String> = emptyList(),
    val dirty: Boolean = false,
    val saving: Boolean = false,
    val saved: Boolean = false,
    val notice: String? = null,
) {
    val isHomeLead: Boolean get() = profile?.serviceLines?.contains("home_move") == true
}

/**
 * The lead's working days and days off. Home-move slots open only where a
 * lead works; this is the calendar the customer's dates are drawn from.
 */
@HiltViewModel
class AvailabilityViewModel @Inject constructor(
    private val api: HomeMoveApiService,
) : ViewModel() {

    private val _uiState = MutableStateFlow(AvailabilityUiState())
    val uiState: StateFlow<AvailabilityUiState> = _uiState.asStateFlow()

    init { load() }

    fun load() {
        viewModelScope.launch {
            _uiState.update { it.copy(loading = true, error = null) }
            runCatching { api.myProvider().data }
                .onSuccess { d ->
                    _uiState.update {
                        it.copy(loading = false, profile = d.profile, workingDays = d.profile.workingDays.sorted(), offDays = d.offDays.sorted(), dirty = false)
                    }
                }
                .onFailure { e ->
                    val http = e as? HttpException
                    val msg = if (http?.code() == 404) {
                        "You aren't onboarded as a provider yet. Operations sets that up."
                    } else {
                        serverMessage(http?.response()?.errorBody()?.string(), e.message ?: "Couldn't load your availability")
                    }
                    _uiState.update { it.copy(loading = false, error = msg) }
                }
        }
    }

    fun toggle(day: Int) = _uiState.update { it.copy(workingDays = toggleDay(it.workingDays, day), dirty = true, saved = false, notice = null) }

    fun addOff(day: LocalDate, today: LocalDate = LocalDate.now()) {
        val problem = offDayProblem(day, today)
        _uiState.update {
            if (problem != null) it.copy(notice = problem)
            else it.copy(offDays = addOffDay(it.offDays, day), dirty = true, saved = false, notice = null)
        }
    }

    fun removeOff(day: String) = _uiState.update { it.copy(offDays = it.offDays - day, dirty = true, saved = false, notice = null) }

    fun save() {
        val s = _uiState.value
        if (s.saving || !s.dirty) return
        viewModelScope.launch {
            _uiState.update { it.copy(saving = true, notice = null) }
            runCatching { api.setAvailability(AvailabilityRequest(s.workingDays, s.offDays)).data }
                .onSuccess { d ->
                    _uiState.update { it.copy(saving = false, saved = true, dirty = false, workingDays = d.workingDays, offDays = d.offDays) }
                }
                .onFailure { e ->
                    val body = (e as? HttpException)?.response()?.errorBody()?.string()
                    _uiState.update { it.copy(saving = false, notice = serverMessage(body, e.message ?: "Didn't save")) }
                }
        }
    }
}
