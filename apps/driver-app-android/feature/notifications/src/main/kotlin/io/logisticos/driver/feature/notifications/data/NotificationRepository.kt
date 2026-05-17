package io.logisticos.driver.feature.notifications.data

import io.logisticos.driver.core.database.dao.NotificationDao
import io.logisticos.driver.core.database.entity.NotificationEntity
import io.logisticos.driver.core.network.auth.SessionManager
import io.logisticos.driver.core.network.service.IdentityApiService
import io.logisticos.driver.core.network.service.RegisterPushTokenRequest
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
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

private fun NotificationEntity.toDriverNotification() = DriverNotification(
    id = id, type = type, title = title, body = body,
    receivedAt = receivedAt, isRead = isRead
)

/**
 * Room-backed notification store.
 *
 * The previous implementation used an in-memory [MutableStateFlow] which was
 * cleared every time the process was killed — the notification bell was always
 * empty on cold start even if the driver had unread messages. Persisting to
 * Room means the badge count and message list survive app restarts, which is
 * the behaviour drivers expect from any other messaging app.
 *
 * Pruning: we keep the 100 most-recent notifications to cap DB growth.
 */
@Singleton
class NotificationRepository @Inject constructor(
    @Named("application_scope") private val scope: CoroutineScope,
    private val identityApiService: IdentityApiService,
    private val sessionManager: SessionManager,
    private val notificationDao: NotificationDao,
) {
    /**
     * Room-reactive list of notifications sorted newest-first.
     * Backed as a [StateFlow] so collectors get an immediate value on
     * subscription (same contract as the old MutableStateFlow).
     */
    val notifications: StateFlow<List<DriverNotification>> =
        notificationDao.getAll()
            .map { entities -> entities.map { it.toDriverNotification() } }
            .stateIn(scope, SharingStarted.Eagerly, emptyList())

    fun saveNotification(type: String, title: String, body: String) {
        val entity = NotificationEntity(
            id         = "${System.currentTimeMillis()}",
            type       = type,
            title      = title,
            body       = body,
            receivedAt = System.currentTimeMillis()
        )
        scope.launch {
            notificationDao.insert(entity)
            // Prune after every insert — cheap because the 100-row window is
            // small and the query runs on a background thread via Room.
            notificationDao.pruneOld()
        }
    }

    fun markAllRead() {
        scope.launch { notificationDao.markAllRead() }
    }

    fun registerFcmToken(token: String) {
        if (!sessionManager.isLoggedIn()) return // not logged in yet; token registered after auth
        scope.launch {
            try {
                identityApiService.registerPushToken(RegisterPushTokenRequest(token = token))
            } catch (_: Exception) {
                // Fire-and-forget: token registration failure is non-fatal.
                // On next app start / token refresh, onNewToken fires again.
            }
        }
    }

    /** Synchronous accessor for the current unread count (reads latest StateFlow value). */
    val unreadCount: Int get() = notifications.value.count { !it.isRead }
}
