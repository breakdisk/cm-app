package io.logisticos.driver.feature.profile.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.logisticos.driver.core.network.auth.SessionManager

private val ProfileCanvas = Color(0xFF050810)
private val ProfileRed = Color(0xFFFF3B5C)
private val ProfileGlass = Color(0x0AFFFFFF)
private val ProfileBorder = Color(0x14FFFFFF)

@Composable
fun ProfileScreen(
    sessionManager: SessionManager,
    isOfflineMode: Boolean,
    onLogout: () -> Unit
) {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(ProfileCanvas)
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp)
    ) {
        Text("Profile", color = Color.White, fontSize = 22.sp, fontWeight = FontWeight.Bold)

        Card(
            modifier = Modifier.fillMaxWidth(),
            colors = CardDefaults.cardColors(containerColor = ProfileGlass),
            border = BorderStroke(1.dp, ProfileBorder)
        ) {
            Column(
                modifier = Modifier.padding(20.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp)
            ) {
                Text("Driver ID", color = Color.White.copy(alpha = 0.5f), fontSize = 12.sp)
                Text("Logged in", color = Color.White, fontSize = 15.sp)
                Text(
                    "Tenant: ${sessionManager.getTenantId() ?: "—"}",
                    color = Color.White.copy(alpha = 0.6f),
                    fontSize = 13.sp
                )
            }
        }

        if (isOfflineMode) {
            Card(
                modifier = Modifier.fillMaxWidth(),
                colors = CardDefaults.cardColors(
                    containerColor = Color(0xFFFFAB00).copy(alpha = 0.1f)
                ),
                border = BorderStroke(1.dp, Color(0xFFFFAB00).copy(alpha = 0.3f))
            ) {
                Text(
                    "Offline Mode Active — profile changes disabled",
                    color = Color(0xFFFFAB00),
                    fontSize = 13.sp,
                    modifier = Modifier.padding(16.dp)
                )
            }
        }

        Spacer(modifier = Modifier.weight(1f))

        Button(
            onClick = onLogout,
            enabled = !isOfflineMode,
            modifier = Modifier
                .fillMaxWidth()
                .height(52.dp),
            colors = ButtonDefaults.buttonColors(containerColor = ProfileRed.copy(alpha = 0.15f)),
            border = BorderStroke(1.dp, ProfileRed.copy(alpha = 0.4f))
        ) {
            Text("Log Out", color = ProfileRed, fontWeight = FontWeight.Bold)
        }
    }
}
