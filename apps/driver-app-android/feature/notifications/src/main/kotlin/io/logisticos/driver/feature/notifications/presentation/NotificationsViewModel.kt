package io.logisticos.driver.feature.notifications.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import io.logisticos.driver.feature.notifications.data.DriverNotification
import io.logisticos.driver.feature.notifications.data.NotificationRepository
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import javax.inject.Inject

/**
 * Thin HiltViewModel wrapper so NotificationsScreen can be used inside a Hilt-managed
 * NavGraph without requiring direct repository injection at the composable call-site.
 * The repository itself is @Singleton so no data is duplicated.
 */
@HiltViewModel
class NotificationsViewModel @Inject constructor(
    private val repository: NotificationRepository
) : ViewModel() {
    val notifications: StateFlow<List<DriverNotification>> = repository.notifications

    val unreadCount: StateFlow<Int> = repository.notifications
        .map { list -> list.count { !it.isRead } }
        .stateIn(viewModelScope, SharingStarted.Eagerly, 0)

    fun markAllRead() = repository.markAllRead()
}
