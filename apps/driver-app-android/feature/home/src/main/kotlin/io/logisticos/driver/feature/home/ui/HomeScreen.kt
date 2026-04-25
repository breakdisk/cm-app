package io.logisticos.driver.feature.home.ui

import android.Manifest
import android.content.pm.PackageManager
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.content.ContextCompat
import androidx.hilt.navigation.compose.hiltViewModel
import io.logisticos.driver.feature.home.presentation.HomeViewModel

private val Canvas = Color(0xFF050810)
private val Cyan = Color(0xFF00E5FF)
private val Amber = Color(0xFFFFAB00)
private val Green = Color(0xFF00FF88)
private val Glass = Color(0x0AFFFFFF)
private val Border = Color(0x14FFFFFF)

@Composable
fun HomeScreen(
    onNavigateToRoute: () -> Unit,
    viewModel: HomeViewModel = hiltViewModel()
) {
    val state by viewModel.uiState.collectAsState()

    // Permissions checklist on Home entry:
    //   ACCESS_FINE / COARSE_LOCATION — dispatch proximity scoring + GPS
    //     heartbeat. Without a runtime grant, FusedLocationProvider throws
    //     SecurityException and pushFreshLocation silently no-ops, leaving
    //     driver_locations empty and the driver un-discoverable by dispatch.
    //   POST_NOTIFICATIONS (API 33+) — FCM pushes for new task assignments
    //     fail silently without it. Driver wouldn't know about new dispatches
    //     until they manually pull-to-refresh.
    //   ACCESS_BACKGROUND_LOCATION — must be requested SEPARATELY after
    //     foreground is granted (Android 10+ rule). Skipped here in the
    //     initial bundle; requested at end of LaunchedEffect once foreground
    //     is confirmed.
    val context = LocalContext.current
    val permissionLauncher = rememberLauncherForActivityResult(
        contract = ActivityResultContracts.RequestMultiplePermissions()
    ) { grants ->
        val fineGranted = grants[Manifest.permission.ACCESS_FINE_LOCATION] == true
        val coarseGranted = grants[Manifest.permission.ACCESS_COARSE_LOCATION] == true
        if (fineGranted || coarseGranted) {
            viewModel.onLocationPermissionGranted()
        } else if (grants.containsKey(Manifest.permission.ACCESS_FINE_LOCATION)
                || grants.containsKey(Manifest.permission.ACCESS_COARSE_LOCATION)) {
            // Driver was prompted for location specifically and said no —
            // surface a rationale card. (Skip when only POST_NOTIFICATIONS
            // was requested.)
            viewModel.onLocationPermissionDenied()
        }
        // POST_NOTIFICATIONS not actionable from the VM yet; the FCM token
        // pipeline (DriverMessagingService) handles delivery once granted.
    }
    val backgroundLocationLauncher = rememberLauncherForActivityResult(
        contract = ActivityResultContracts.RequestPermission()
    ) { _ /* granted */ ->
        // Background grant is best-effort. If denied, GPS pings stop when
        // the screen locks; foreground heartbeat still works. No retry —
        // the OS auto-denies subsequent prompts after one rejection.
    }
    LaunchedEffect(Unit) {
        val fine = ContextCompat.checkSelfPermission(
            context, Manifest.permission.ACCESS_FINE_LOCATION
        ) == PackageManager.PERMISSION_GRANTED
        val coarse = ContextCompat.checkSelfPermission(
            context, Manifest.permission.ACCESS_COARSE_LOCATION
        ) == PackageManager.PERMISSION_GRANTED

        // Build the initial bundle: foreground location always, +
        // POST_NOTIFICATIONS on API 33+.
        val needed = mutableListOf<String>()
        if (!fine && !coarse) {
            needed += Manifest.permission.ACCESS_FINE_LOCATION
            needed += Manifest.permission.ACCESS_COARSE_LOCATION
        }
        if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.TIRAMISU) {
            val notif = ContextCompat.checkSelfPermission(
                context, Manifest.permission.POST_NOTIFICATIONS
            ) == PackageManager.PERMISSION_GRANTED
            if (!notif) needed += Manifest.permission.POST_NOTIFICATIONS
        }

        if (needed.isNotEmpty()) {
            permissionLauncher.launch(needed.toTypedArray())
        } else {
            viewModel.onLocationPermissionGranted()
        }

        // Background-location escalation, after foreground is granted.
        // Android 10+ rule: cannot bundle background with foreground in one
        // dialog — must be a follow-up request, and the OS shows a separate
        // settings-style screen rather than a popup.
        if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.Q) {
            val foregroundOk = ContextCompat.checkSelfPermission(
                context, Manifest.permission.ACCESS_FINE_LOCATION
            ) == PackageManager.PERMISSION_GRANTED
            val backgroundOk = ContextCompat.checkSelfPermission(
                context, Manifest.permission.ACCESS_BACKGROUND_LOCATION
            ) == PackageManager.PERMISSION_GRANTED
            if (foregroundOk && !backgroundOk) {
                backgroundLocationLauncher.launch(Manifest.permission.ACCESS_BACKGROUND_LOCATION)
            }
        }
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(Canvas)
            .verticalScroll(rememberScrollState())
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp)
    ) {
        // ── Online / Offline toggle ───────────────────────────────────────────
        val statusColor = if (state.isOnline) Green else Color.White.copy(alpha = 0.4f)
        val statusLabel = if (state.isOnline) "ONLINE" else "OFFLINE"
        Card(
            modifier = Modifier.fillMaxWidth(),
            shape = RoundedCornerShape(16.dp),
            colors = CardDefaults.cardColors(
                containerColor = if (state.isOnline) Green.copy(alpha = 0.10f) else Glass
            ),
            border = androidx.compose.foundation.BorderStroke(
                1.dp,
                if (state.isOnline) Green.copy(alpha = 0.5f) else Border
            )
        ) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 20.dp, vertical = 16.dp),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically
            ) {
                Column {
                    Text(
                        text = statusLabel,
                        color = statusColor,
                        fontSize = 18.sp,
                        fontWeight = FontWeight.Bold,
                        letterSpacing = 2.sp
                    )
                    Text(
                        text = if (state.isOnline) "Accepting jobs" else "Not accepting jobs",
                        color = Color.White.copy(alpha = 0.5f),
                        fontSize = 12.sp
                    )
                }
                if (state.isTogglingStatus) {
                    CircularProgressIndicator(
                        color = if (state.isOnline) Green else Cyan,
                        modifier = Modifier.size(28.dp),
                        strokeWidth = 2.dp
                    )
                } else {
                    Switch(
                        checked = state.isOnline,
                        onCheckedChange = { viewModel.toggleOnlineStatus() },
                        colors = SwitchDefaults.colors(
                            checkedThumbColor = Canvas,
                            checkedTrackColor = Green,
                            uncheckedThumbColor = Color.White.copy(alpha = 0.6f),
                            uncheckedTrackColor = Color.White.copy(alpha = 0.15f)
                        )
                    )
                }
            }
        }

        // Pending sync items \u2014 silent retries get a visible signal so the
        // driver knows a POD/scan/COD entry hasn't yet hit the server.
        if (state.pendingSyncCount > 0) {
            Card(
                modifier = Modifier.fillMaxWidth(),
                colors = CardDefaults.cardColors(containerColor = Cyan.copy(alpha = 0.10f)),
                border = androidx.compose.foundation.BorderStroke(1.dp, Cyan.copy(alpha = 0.35f))
            ) {
                Row(
                    modifier = Modifier.padding(12.dp),
                    horizontalArrangement = Arrangement.spacedBy(10.dp),
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    CircularProgressIndicator(
                        color = Cyan,
                        modifier = Modifier.size(14.dp),
                        strokeWidth = 1.5.dp,
                    )
                    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                        Text(
                            text = if (state.pendingSyncCount == 1) "1 item syncing"
                                   else "${state.pendingSyncCount} items syncing",
                            color = Cyan, fontSize = 13.sp, fontWeight = FontWeight.Medium
                        )
                        Text(
                            "Will retry automatically when online",
                            color = Color.White.copy(alpha = 0.5f), fontSize = 11.sp
                        )
                    }
                }
            }
        }

        // Location permission denied \u2014 link to OS settings; the OS won't show
        // an in-app prompt again after one rejection on Android 11+.
        if (state.locationDenied) {
            Card(
                modifier = Modifier.fillMaxWidth(),
                colors = CardDefaults.cardColors(containerColor = Color(0xFFFF3B5C).copy(alpha = 0.12f)),
                border = androidx.compose.foundation.BorderStroke(1.dp, Color(0xFFFF3B5C).copy(alpha = 0.40f))
            ) {
                Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(
                        "Location access required",
                        color = Color(0xFFFF3B5C),
                        fontSize = 13.sp,
                        fontWeight = FontWeight.SemiBold,
                    )
                    Text(
                        "Dispatch can't see you without location. " +
                            "Open Settings to grant access \u2014 without it you won't receive new tasks.",
                        color = Color.White.copy(alpha = 0.6f), fontSize = 12.sp
                    )
                    TextButton(
                        onClick = {
                            val intent = android.content.Intent(
                                android.provider.Settings.ACTION_APPLICATION_DETAILS_SETTINGS,
                                android.net.Uri.fromParts("package", context.packageName, null),
                            )
                            context.startActivity(intent)
                        },
                        contentPadding = androidx.compose.foundation.layout.PaddingValues(horizontal = 8.dp, vertical = 4.dp),
                    ) {
                        Text("Open Settings", color = Color(0xFFFF3B5C), fontSize = 12.sp, fontWeight = FontWeight.Medium)
                    }
                }
            }
        }

        if (state.isOfflineMode) {
            Card(
                modifier = Modifier.fillMaxWidth(),
                colors = CardDefaults.cardColors(containerColor = Amber.copy(alpha = 0.15f)),
                border = androidx.compose.foundation.BorderStroke(1.dp, Amber.copy(alpha = 0.4f))
            ) {
                Row(
                    modifier = Modifier.padding(12.dp),
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Text("\u26a0", fontSize = 16.sp)
                    Text(
                        "Offline Mode Active — reconnect to sync",
                        color = Amber, fontSize = 13.sp, fontWeight = FontWeight.Medium
                    )
                }
            }
        }

        state.error?.let { err ->
            Card(
                modifier = Modifier.fillMaxWidth(),
                colors = CardDefaults.cardColors(containerColor = Color(0xFFFF3B5C).copy(alpha = 0.12f)),
                border = androidx.compose.foundation.BorderStroke(1.dp, Color(0xFFFF3B5C).copy(alpha = 0.4f))
            ) {
                Text(
                    text = err,
                    color = Color(0xFFFF3B5C),
                    fontSize = 12.sp,
                    modifier = Modifier.padding(12.dp)
                )
            }
        }

        val shift = state.shift
        Card(
            modifier = Modifier.fillMaxWidth(),
            colors = CardDefaults.cardColors(containerColor = Glass),
            border = androidx.compose.foundation.BorderStroke(1.dp, Border)
        ) {
            Column(modifier = Modifier.padding(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Text("Today's Shift", color = Color.White.copy(alpha = 0.6f), fontSize = 13.sp)
                    IconButton(
                        onClick = { viewModel.syncShift() },
                        modifier = Modifier.size(28.dp)
                    ) {
                        Icon(
                            imageVector = Icons.Default.Refresh,
                            contentDescription = "Refresh",
                            tint = Cyan.copy(alpha = 0.7f),
                            modifier = Modifier.size(18.dp)
                        )
                    }
                }
                if (shift != null) {
                    Row(horizontalArrangement = Arrangement.spacedBy(24.dp)) {
                        StatItem(label = "Total", value = shift.totalStops.toString(), color = Color.White)
                        StatItem(label = "Done", value = shift.completedStops.toString(), color = Green)
                        StatItem(label = "Failed", value = shift.failedStops.toString(), color = Color(0xFFFF3B5C))
                        StatItem(label = "COD", value = "\u20b1${shift.totalCodCollected.toInt()}", color = Cyan)
                    }
                } else if (state.isLoading) {
                    CircularProgressIndicator(color = Cyan, modifier = Modifier.size(24.dp))
                } else {
                    Text("No active shift", color = Color.White.copy(alpha = 0.4f), fontSize = 14.sp)
                }
            }
        }

        Button(
            onClick = onNavigateToRoute,
            enabled = shift != null,
            modifier = Modifier
                .fillMaxWidth()
                .height(52.dp),
            colors = ButtonDefaults.buttonColors(containerColor = Cyan)
        ) {
            Text("View Route", color = Canvas, fontWeight = FontWeight.Bold, fontSize = 16.sp)
        }
    }
}

@Composable
private fun StatItem(label: String, value: String, color: Color) {
    Column(horizontalAlignment = Alignment.CenterHorizontally) {
        Text(value, color = color, fontSize = 22.sp, fontWeight = FontWeight.Bold)
        Text(label, color = Color.White.copy(alpha = 0.5f), fontSize = 11.sp)
    }
}
