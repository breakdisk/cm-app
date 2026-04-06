package io.logisticos.driver.feature.notifications.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.logisticos.driver.feature.notifications.data.DriverNotification
import io.logisticos.driver.feature.notifications.presentation.NotificationsViewModel

private val NotifCanvas = Color(0xFF050810)
private val NotifCyan = Color(0xFF00E5FF)
private val NotifGlass = Color(0x0AFFFFFF)
private val NotifBorder = Color(0x14FFFFFF)

@Composable
fun NotificationsScreen(
    viewModel: NotificationsViewModel
) {
    val notifications by viewModel.notifications.collectAsState()

    LaunchedEffect(Unit) { viewModel.markAllRead() }

    Column(modifier = Modifier.fillMaxSize().background(NotifCanvas)) {
        Text(
            "Notifications",
            color = Color.White,
            fontSize = 22.sp,
            fontWeight = FontWeight.Bold,
            modifier = Modifier.padding(16.dp)
        )

        if (notifications.isEmpty()) {
            Box(modifier = Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                Text("No notifications", color = Color.White.copy(alpha = 0.3f), fontSize = 14.sp)
            }
        } else {
            LazyColumn(
                contentPadding = PaddingValues(horizontal = 16.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp)
            ) {
                items(notifications) { notification ->
                    NotificationCard(notification)
                }
            }
        }
    }
}

@Composable
private fun NotificationCard(notification: DriverNotification) {
    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(containerColor = NotifGlass),
        border = androidx.compose.foundation.BorderStroke(1.dp, NotifBorder)
    ) {
        Column(
            modifier = Modifier.padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp)
        ) {
            Row(
                horizontalArrangement = Arrangement.SpaceBetween,
                modifier = Modifier.fillMaxWidth()
            ) {
                Text(
                    notification.title,
                    color = Color.White,
                    fontWeight = FontWeight.Medium,
                    fontSize = 15.sp
                )
                if (!notification.isRead) {
                    Box(
                        modifier = Modifier
                            .size(8.dp)
                            .background(NotifCyan, shape = MaterialTheme.shapes.small)
                    )
                }
            }
            Text(
                notification.body,
                color = Color.White.copy(alpha = 0.6f),
                fontSize = 13.sp
            )
            Text(
                formatTime(notification.receivedAt),
                color = Color.White.copy(alpha = 0.3f),
                fontSize = 11.sp
            )
        }
    }
}

private fun formatTime(timestamp: Long): String {
    val diff = System.currentTimeMillis() - timestamp
    return when {
        diff < 60_000 -> "Just now"
        diff < 3_600_000 -> "${diff / 60_000}m ago"
        diff < 86_400_000 -> "${diff / 3_600_000}h ago"
        else -> "${diff / 86_400_000}d ago"
    }
}
