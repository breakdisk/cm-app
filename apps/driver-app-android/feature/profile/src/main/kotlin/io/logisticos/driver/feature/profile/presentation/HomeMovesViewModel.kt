package io.logisticos.driver.feature.profile.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import io.logisticos.driver.core.network.service.HomeMoveApiService
import io.logisticos.driver.core.network.service.LeadMoveItem
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import retrofit2.HttpException
import javax.inject.Inject

data class HomeMovesUiState(
    val loading: Boolean = false,
    val moves: List<LeadMoveItem> = emptyList(),
    val error: String? = null,
)

/** The lead's reserved whole-home moves, soonest first (dispatch orders them). */
@HiltViewModel
class HomeMovesViewModel @Inject constructor(
    private val api: HomeMoveApiService,
) : ViewModel() {

    private val _uiState = MutableStateFlow(HomeMovesUiState())
    val uiState: StateFlow<HomeMovesUiState> = _uiState.asStateFlow()

    fun load() {
        viewModelScope.launch {
            _uiState.update { it.copy(loading = true, error = null) }
            runCatching { api.myReservations() }
                .onSuccess { res -> _uiState.update { it.copy(loading = false, moves = res.data) } }
                .onFailure { e ->
                    val body = (e as? HttpException)?.response()?.errorBody()?.string()
                    _uiState.update { it.copy(loading = false, error = serverMessage(body, e.message ?: "Couldn't load your moves")) }
                }
        }
    }
}
