package io.logisticos.driver.feature.notifications.presentation

import androidx.lifecycle.ViewModel
import dagger.hilt.android.lifecycle.HiltViewModel
import io.logisticos.driver.feature.notifications.data.NotificationRepository
import javax.inject.Inject

/**
 * Thin HiltViewModel wrapper so NotificationsScreen can be used inside a Hilt-managed
 * NavGraph without requiring direct repository injection at the composable call-site.
 * The repository itself is @Singleton so no data is duplicated.
 */
@HiltViewModel
class NotificationsViewModel @Inject constructor(
    val repository: NotificationRepository
) : ViewModel()
