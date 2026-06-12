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
                            merchantName     = message.data["merchant_name"]     ?: "",
                            deliveryCategory = message.data["delivery_category"] ?: "parcel",
                            weightGrams      = message.data["weight_grams"]?.toLongOrNull() ?: 0L,
                            pickupLat        = message.data["pickup_lat"]?.toDoubleOrNull(),
                            pickupLng        = message.data["pickup_lng"]?.toDoubleOrNull(),
                            deliveryLat      = message.data["delivery_lat"]?.toDoubleOrNull(),
                            deliveryLng      = message.data["delivery_lng"]?.toDoubleOrNull(),
                        )
                    )
                }
                // Still sync task list so RouteScreen is up to date after accept.
                TaskSyncBus.requestSync()
            }
            // Broadcast gig offer — contended; rendered as a GRAB card with a
            // countdown. assignmentId stays blank: no assignment exists until
            // a claim wins.
            "task_offer" -> {
                val offerId = message.data["offer_id"] ?: ""
                if (offerId.isNotBlank()) {
                    PendingAssignmentBus.post(
                        AssignmentPayload(
                            assignmentId   = "",
                            shipmentId     = message.data["shipment_id"]     ?: "",
                            customerName   = message.data["customer_name"]   ?: "Unknown Customer",
                            address        = message.data["address"]         ?: "",
                            taskType       = "delivery",
                            trackingNumber = message.data["tracking_number"] ?: "",
                            codAmountCents = message.data["cod_amount_cents"]?.toLongOrNull() ?: 0L,
                            merchantName     = message.data["merchant_name"]     ?: "",
                            deliveryCategory = message.data["delivery_category"] ?: "parcel",
                            weightGrams      = message.data["weight_grams"]?.toLongOrNull() ?: 0L,
                            pickupLat        = message.data["pickup_lat"]?.toDoubleOrNull(),
                            pickupLng        = message.data["pickup_lng"]?.toDoubleOrNull(),
                            deliveryLat      = message.data["delivery_lat"]?.toDoubleOrNull(),
                            deliveryLng      = message.data["delivery_lng"]?.toDoubleOrNull(),
                            offerId          = offerId,
                            payoutCents      = message.data["payout_cents"]?.toLongOrNull(),
                            expiresAtMillis  = message.data["expires_at_ms"]?.toLongOrNull(),
                        )
                    )
                }
            }
            // Another driver won the race (or the offer expired) — flip the
            // on-screen grab card to "Taken" / dismiss.
            "offer_closed" -> {
                val offerId = message.data["offer_id"] ?: ""
                if (offerId.isNotBlank()) PendingAssignmentBus.markTaken(offerId)
            }
            "dispatch_message" -> TaskSyncBus.requestSync()
        }

        // offer_closed is in-app state only — a tray notification for every
        // lost race would be pure noise.
        if (type != "offer_closed") {
            showSystemNotification(title, body, type, message.data)
        }
    }

    override fun onNewToken(token: String) {
        notificationRepo.registerFcmToken(token)
    }

    private fun showSystemNotification(title: String, body: String, type: String, data: Map<String, String> = emptyMap()) {
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
            // Include all FCM data fields so MainActivity can re-post the assignment
            // payload to PendingAssignmentBus when the app is launched cold from the tray.
            data.forEach { (key, value) -> putExtra(key, value) }
            flags = Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP
        }
        val pendingIntent = PendingIntent.getActivity(
            this, notificationIdCounter.get(), intent,
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
