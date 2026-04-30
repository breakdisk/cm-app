package io.logisticos.driver.feature.notifications

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Intent
import androidx.core.app.NotificationCompat
import com.google.firebase.messaging.FirebaseMessagingService
import com.google.firebase.messaging.RemoteMessage
import dagger.hilt.android.AndroidEntryPoint
import io.logisticos.driver.core.common.AssignmentPayload
import io.logisticos.driver.core.common.PendingAssignmentBus
import io.logisticos.driver.core.common.TaskSyncBus
import io.logisticos.driver.feature.notifications.data.NotificationRepository
import java.util.concurrent.atomic.AtomicInteger
import javax.inject.Inject

@AndroidEntryPoint
class DriverMessagingService : FirebaseMessagingService() {

    companion object {
        private val notificationIdCounter = AtomicInteger(1)
    }

    @Inject lateinit var notificationRepo: NotificationRepository

    override fun onMessageReceived(message: RemoteMessage) {
        val type  = message.data["type"] ?: "dispatch_message"
        val title = message.notification?.title ?: message.data["title"] ?: "LogisticOS"
        val body  = message.notification?.body  ?: message.data["body"]  ?: ""

        notificationRepo.saveNotification(type = type, title = title, body = body)

        when (type) {
            "task_assigned" -> {
                // Extract assignment payload from FCM data map.
                // The backend must include these fields; missing fields fall back to safe
                // defaults so the AssignmentScreen still renders rather than crashing.
                val assignmentId = message.data["assignment_id"] ?: ""
                if (assignmentId.isNotBlank()) {
                    PendingAssignmentBus.post(
                        AssignmentPayload(
                            assignmentId   = assignmentId,
                            shipmentId     = message.data["shipment_id"]     ?: "",
                            customerName   = message.data["customer_name"]   ?: "Unknown Customer",
                            address        = message.data["address"]         ?: "",
                            taskType       = message.data["task_type"]       ?: "delivery",
                            trackingNumber = message.data["tracking_number"] ?: "",
                            codAmountCents = message.data["cod_amount_cents"]?.toLongOrNull() ?: 0L,
                        )
                    )
                }
                // Still sync task list so RouteScreen is up to date after accept.
                TaskSyncBus.requestSync()
            }
            "dispatch_message" -> TaskSyncBus.requestSync()
        }

        showSystemNotification(title, body, type)
    }

    override fun onNewToken(token: String) {
        notificationRepo.registerFcmToken(token)
    }

    private fun showSystemNotification(title: String, body: String, type: String) {
        val channelId = "driver_notifications"
        val notificationManager = getSystemService(NotificationManager::class.java)

        if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.O) {
            notificationManager.createNotificationChannel(
                NotificationChannel(channelId, "Driver Alerts", NotificationManager.IMPORTANCE_HIGH)
            )
        }

        val intent = Intent().apply {
            setClassName(packageName, "$packageName.MainActivity")
            putExtra("notification_type", type)
            flags = Intent.FLAG_ACTIVITY_SINGLE_TOP
        }
        val pendingIntent = PendingIntent.getActivity(
            this, 0, intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        )

        val notification = NotificationCompat.Builder(this, channelId)
            .setContentTitle(title)
            .setContentText(body)
            .setSmallIcon(R.drawable.ic_notification)
            .setAutoCancel(true)
            .setContentIntent(pendingIntent)
            .setPriority(NotificationCompat.PRIORITY_HIGH)
            .build()

        notificationManager.notify(notificationIdCounter.getAndIncrement(), notification)
    }
}
