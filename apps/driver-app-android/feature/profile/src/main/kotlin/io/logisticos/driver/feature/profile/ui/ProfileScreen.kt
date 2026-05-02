package io.logisticos.driver.feature.profile.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowForward
import androidx.compose.material.icons.filled.VerifiedUser
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
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
private val ProfileAmber = Color(0xFFFFAB00)
private val ProfileCyan = Color(0xFF00E5FF)

@Composable
fun ProfileScreen(
    sessionManager: SessionManager,
    isOfflineMode: Boolean,
    onNavigateToCompliance: () -> Unit = {},
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
                Text(
                    sessionManager.getDriverId() ?: "—",
                    color = Color.White,
                    fontSize = 15.sp
                )
                Text(
                    "Tenant: ${sessionManager.getTenantId() ?: "—"}",
                    color = Color.White.copy(alpha = 0.6f),
                    fontSize = 13.sp
                )
            }
        }

        Card(
            modifier = Modifier
                .fillMaxWidth()
                .clickable(enabled = !isOfflineMode, onClick = onNavigateToCompliance),
            colors = CardDefaults.cardColors(containerColor = ProfileGlass),
            border = BorderStroke(1.dp, ProfileCyan.copy(alpha = 0.3f))
        ) {
            Row(
                modifier = Modifier.padding(20.dp).fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically
            ) {
                Row(
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Icon(
                        imageVector = Icons.Filled.VerifiedUser,
                        contentDescription = null,
                        tint = ProfileCyan,
                        modifier = Modifier.size(24.dp)
                    )
                    Column {
                        Text(
                            "Verification Documents",
                            color = Color.White,
                            fontSize = 15.sp,
                            fontWeight = FontWeight.SemiBold
                        )
                        Text(
                            "License, ID, vehicle registration",
                            color = Color.White.copy(alpha = 0.5f),
                            fontSize = 12.sp
                        )
                    }
                }
                Icon(
                    imageVector = Icons.AutoMirrored.Filled.ArrowForward,
                    contentDescription = null,
                    tint = Color.White.copy(alpha = 0.4f),
                    modifier = Modifier.size(20.dp)
                )
            }
        }

        if (isOfflineMode) {
            Card(
                modifier = Modifier.fillMaxWidth(),
                colors = CardDefaults.cardColors(
                    containerColor = ProfileAmber.copy(alpha = 0.1f)
                ),
                border = BorderStroke(1.dp, ProfileAmber.copy(alpha = 0.3f))
            ) {
                Text(
                    "Offline Mode Active — profile changes disabled",
                    color = ProfileAmber,
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
