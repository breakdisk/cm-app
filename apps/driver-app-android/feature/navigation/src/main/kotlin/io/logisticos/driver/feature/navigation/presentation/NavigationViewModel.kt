package io.logisticos.driver.feature.navigation.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import dagger.hilt.android.lifecycle.HiltViewModel
import io.logisticos.driver.core.database.dao.TaskDao
import io.logisticos.driver.core.database.entity.RouteEntity
import io.logisticos.driver.core.database.entity.TaskEntity
import io.logisticos.driver.feature.navigation.data.NavigationRepository
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

data class NavigationUiState(
    val task: TaskEntity? = null,
    val route: RouteEntity? = null,
    val currentLat: Double = 0.0,
    val currentLng: Double = 0.0,
    val currentBearing: Float = 0f,
    val nextInstruction: String = "",
    val distanceToNextM: Int = 0,
    val isArrived: Boolean = false,
    val isLoading: Boolean = false
)

@HiltViewModel(assistedFactory = NavigationViewModel.Factory::class)
class NavigationViewModel @AssistedInject constructor(
    private val navRepo: NavigationRepository,
    private val taskDao: TaskDao,
    @Assisted private val taskId: String
) : ViewModel() {

    @AssistedFactory
    interface Factory {
        fun create(taskId: String): NavigationViewModel
    }

    private val _uiState = MutableStateFlow(NavigationUiState())
    val uiState: StateFlow<NavigationUiState> = _uiState.asStateFlow()

    init {
        viewModelScope.launch {
            val task = taskDao.getById(taskId)
            _uiState.update { it.copy(task = task) }
        }
        viewModelScope.launch {
            navRepo.observeRoute(taskId).collect { route ->
                _uiState.update { it.copy(route = route) }
            }
        }
    }

    fun updateLocation(lat: Double, lng: Double, bearing: Float) {
        _uiState.update { it.copy(currentLat = lat, currentLng = lng, currentBearing = bearing) }
        checkArrival(lat, lng)
    }

    private fun checkArrival(lat: Double, lng: Double) {
        val task = _uiState.value.task ?: return
        val distance = haversineMeters(lat, lng, task.lat, task.lng)
        if (distance < 50.0) _uiState.update { it.copy(isArrived = true) }
    }

    fun fetchRoute(originLat: Double, originLng: Double) {
        val task = _uiState.value.task ?: return
        viewModelScope.launch {
            _uiState.update { it.copy(isLoading = true) }
            runCatching {
                navRepo.fetchRoute(taskId, originLat, originLng, task.lat, task.lng)
            }
            _uiState.update { it.copy(isLoading = false) }
        }
    }

    private fun haversineMeters(
        lat1: Double, lng1: Double,
        lat2: Double, lng2: Double
    ): Double {
        val r = 6_371_000.0
        val dLat = Math.toRadians(lat2 - lat1)
        val dLng = Math.toRadians(lng2 - lng1)
        val a = Math.sin(dLat / 2).let { it * it } +
                Math.cos(Math.toRadians(lat1)) * Math.cos(Math.toRadians(lat2)) *
                Math.sin(dLng / 2).let { it * it }
        return r * 2 * Math.atan2(Math.sqrt(a), Math.sqrt(1 - a))
    }
}
