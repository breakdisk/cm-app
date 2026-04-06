package io.logisticos.driver.feature.profile.presentation

import androidx.lifecycle.ViewModel
import dagger.hilt.android.lifecycle.HiltViewModel
import io.logisticos.driver.core.network.auth.SessionManager
import javax.inject.Inject

/**
 * Thin HiltViewModel wrapper that exposes the SessionManager and offline-mode state
 * to ProfileScreen without requiring direct injection at the composable call-site.
 */
@HiltViewModel
class ProfileViewModel @Inject constructor(
    val sessionManager: SessionManager
) : ViewModel() {
    val isOfflineMode: Boolean get() = sessionManager.isOfflineModeActive()
}
