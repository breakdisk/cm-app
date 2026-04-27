package io.logisticos.driver.feature.notifications.data

import io.logisticos.driver.core.network.auth.SessionManager
import io.logisticos.driver.core.network.service.FcmTokenRequest
import io.logisticos.driver.core.network.service.IdentityApiService
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import javax.inject.Inject
import javax.inject.Named
import javax.inject.Singleton

data class DriverNotification(
    val id: String,
    val type: String,
    val title: String,
    val body: String,
    val receivedAt: Long,
    val isRead: Boolean = false
)

@Singleton
class NotificationRepository @Inject constructor(
    @Named("application_scope") private val scope: CoroutineScope,
    private val identityApiService: IdentityApiService,
    private val sessionManager: SessionManager
) {
    private val _notifications = MutableStateFlow<List<DriverNotification>>(emptyList())
    val notifications: StateFlow<List<DriverNotification>> = _notifications.asStateFlow()

    fun saveNotification(type: String, title: String, body: String) {
        val notification = DriverNotification(
            id = "${System.currentTimeMillis()}",
            type = type,
            title = title,
            body = body,
            receivedAt = System.currentTimeMillis()
        )
        _notifications.update { listOf(notification) + it }
    }

    fun markAllRead() {
        _notifications.update { list -> list.map { it.copy(isRead = true) } }
    }

    fun registerFcmToken(token: String) {
        if (!sessionManager.isLoggedIn()) return // not logged in yet; token registered after auth
        scope.launch {
            try {
                identityApiService.registerPushToken(
                    io.logisticos.driver.core.network.service.RegisterPushTokenRequest(token = token)
                )
            } catch (_: Exception) {
                // Fire-and-forget: token registration failure is non-fatal.
                // On next app start / token refresh, onNewToken fires again.
            }
        }
    }

    val unreadCount: Int get() = _notifications.value.count { !it.isRead }
}
