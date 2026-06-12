package io.logisticos.driver.feature.profile.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowForward
import androidx.compose.material.icons.filled.VerifiedUser
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.hilt.navigation.compose.hiltViewModel
import io.logisticos.driver.feature.profile.presentation.ProfileViewModel

private val ProfileCanvas = Color(0xFF050810)
private val ProfileRed    = Color(0xFFFF3B5C)
private val ProfileGlass  = Color(0x0AFFFFFF)
private val ProfileBorder = Color(0x14FFFFFF)
private val ProfileAmber  = Color(0xFFFFAB00)
private val ProfileCyan   = Color(0xFF00E5FF)
private val ProfileGreen  = Color(0xFF00FF88)

private fun pesoLabel(cents: Long): String = "₱${"%,.2f".format(cents / 100.0)}"

@Composable
fun ProfileScreen(
    onNavigateToCompliance: () -> Unit = {},
    onNavigateToEarnings: () -> Unit = {},
    onLogout: () -> Unit,
    viewModel: ProfileViewModel = hiltViewModel()
) {
    val state by viewModel.uiState.collectAsState()

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(ProfileCanvas)
            // Scrollable: identity + financial + compliance cards exceed small
            // viewports (5" / 720p) — weight-based spacing can't be used below.
            .verticalScroll(rememberScrollState())
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp)
    ) {
        Text("Profile", color = Color.White, fontSize = 22.sp, fontWeight = FontWeight.Bold)

        // ── Driver identity card ──────────────────────────────────────────────
        Card(
            modifier = Modifier.fillMaxWidth(),
            colors = CardDefaults.cardColors(containerColor = ProfileGlass),
            border = BorderStroke(1.dp, ProfileBorder)
        ) {
            Column(
                modifier = Modifier.padding(20.dp),
                verticalArrangement = Arrangement.spacedBy(10.dp)
            ) {
                // Avatar initial + name
                Row(
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    // Avatar circle — first letter of display name or "?" while loading
                    val initial = state.displayName.firstOrNull()?.uppercaseChar()?.toString() ?: "?"
                    Box(
                        modifier = Modifier
                            .size(52.dp)
                            .clip(RoundedCornerShape(26.dp))
                            .background(ProfileCyan.copy(alpha = 0.12f)),
                        contentAlignment = Alignment.Center
                    ) {
                        if (state.isLoading) {
                            CircularProgressIndicator(
                                color = ProfileCyan,
                                modifier = Modifier.size(20.dp),
                                strokeWidth = 2.dp
                            )
                        } else {
                            Text(
                                initial,
                                color = ProfileCyan,
                                fontSize = 22.sp,
                                fontWeight = FontWeight.Bold
                            )
                        }
                    }
                    Column {
                        if (state.displayName.isNotBlank()) {
                            Text(
                                state.displayName,
                                color = Color.White,
                                fontSize = 18.sp,
                                fontWeight = FontWeight.Bold
                            )
                        } else if (!state.isLoading) {
                            // Fall back to Driver ID if the API doesn't return a name yet
                            Text(
                                "Driver ${state.driverId.take(8)}",
                                color = Color.White.copy(alpha = 0.7f),
                                fontSize = 15.sp,
                                fontWeight = FontWeight.Medium
                            )
                        }
                        if (state.email.isNotBlank()) {
                            Text(
                                state.email,
                                color = Color.White.copy(alpha = 0.55f),
                                fontSize = 13.sp
                            )
                        }
                    }
                }

                HorizontalDivider(color = ProfileBorder)

                // Phone
                if (state.phone.isNotBlank()) {
                    Row(
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Text("📞", fontSize = 14.sp)
                        Text(
                            state.phone,
                            color = ProfileCyan.copy(alpha = 0.85f),
                            fontSize = 14.sp
                        )
                    }
                }

                // Raw IDs (smaller, secondary row)
                Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                    Text(
                        "Driver ID: ${state.driverId.ifBlank { "—" }}",
                        color = Color.White.copy(alpha = 0.35f),
                        fontSize = 11.sp,
                        fontFamily = FontFamily.Monospace
                    )
                    Text(
                        "Tenant: ${state.tenantId.ifBlank { "—" }}",
                        color = Color.White.copy(alpha = 0.35f),
                        fontSize = 11.sp,
                        fontFamily = FontFamily.Monospace
                    )
                }
            }
        }

        // ── Financial section ─────────────────────────────────────────────────
        // Earnings card: gig (part-time) drivers only — full-time drivers never
        // see payout figures. Reads the contractual payout_cents snapshots.
        if (state.isGigWorker) {
            Card(
                modifier = Modifier
                    .fillMaxWidth()
                    .clickable(enabled = !state.isOfflineMode, onClick = onNavigateToEarnings),
                colors = CardDefaults.cardColors(containerColor = ProfileGlass),
                border = BorderStroke(1.dp, ProfileGreen.copy(alpha = 0.3f))
            ) {
                Row(
                    modifier = Modifier.padding(20.dp).fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        Text(
                            "Earnings",
                            color = Color.White, fontSize = 15.sp, fontWeight = FontWeight.SemiBold
                        )
                        Row(horizontalArrangement = Arrangement.spacedBy(16.dp)) {
                            Column {
                                Text(
                                    pesoLabel(state.todayCents),
                                    color = ProfileGreen, fontSize = 18.sp,
                                    fontWeight = FontWeight.Bold, fontFamily = FontFamily.Monospace
                                )
                                Text("Today", color = Color.White.copy(alpha = 0.45f), fontSize = 11.sp)
                            }
                            Column {
                                Text(
                                    pesoLabel(state.weekCents),
                                    color = ProfileCyan, fontSize = 18.sp,
                                    fontWeight = FontWeight.Bold, fontFamily = FontFamily.Monospace
                                )
                                Text("This week", color = Color.White.copy(alpha = 0.45f), fontSize = 11.sp)
                            }
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
        }

        // Cash-to-remit card: any driver carrying unremitted COD/pickup cash
        // sees what they owe the hub — the #1 driver-support question.
        if (state.openBalanceCents > 0) {
            Card(
                modifier = Modifier
                    .fillMaxWidth()
                    .clickable(enabled = !state.isOfflineMode, onClick = onNavigateToEarnings),
                colors = CardDefaults.cardColors(containerColor = ProfileAmber.copy(alpha = 0.08f)),
                border = BorderStroke(1.dp, ProfileAmber.copy(alpha = 0.4f))
            ) {
                Row(
                    modifier = Modifier.padding(20.dp).fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                        Text(
                            "Cash to remit",
                            color = ProfileAmber, fontSize = 15.sp, fontWeight = FontWeight.SemiBold
                        )
                        Text(
                            if (state.openDebitCount > 0)
                                "from ${state.openDebitCount} ${if (state.openDebitCount == 1) "delivery" else "deliveries"}"
                            else "current shift",
                            color = Color.White.copy(alpha = 0.5f), fontSize = 12.sp
                        )
                    }
                    Text(
                        pesoLabel(state.openBalanceCents),
                        color = ProfileAmber, fontSize = 20.sp,
                        fontWeight = FontWeight.Bold, fontFamily = FontFamily.Monospace
                    )
                }
            }
        }

        // ── Verification documents ────────────────────────────────────────────
        Card(
            modifier = Modifier
                .fillMaxWidth()
                .clickable(enabled = !state.isOfflineMode, onClick = onNavigateToCompliance),
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

        // ── Offline mode banner ───────────────────────────────────────────────
        if (state.isOfflineMode) {
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

        Spacer(modifier = Modifier.height(8.dp))

        Button(
            onClick = onLogout,
            enabled = !state.isOfflineMode,
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
