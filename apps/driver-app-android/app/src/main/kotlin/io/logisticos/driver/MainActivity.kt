package io.logisticos.driver

import android.content.Intent
import android.net.Uri
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.lifecycle.lifecycleScope
import dagger.hilt.android.AndroidEntryPoint
import io.logisticos.driver.core.common.AssignmentPayload
import io.logisticos.driver.core.common.PendingAssignmentBus
import io.logisticos.driver.core.database.worker.OutboundSyncWorker
import io.logisticos.driver.core.network.auth.SessionManager
import io.logisticos.driver.core.network.branding.BrandingStore
import io.logisticos.driver.navigation.AppNavGraph
import io.logisticos.driver.security.RootChecker
import io.logisticos.driver.ui.theme.DriverAppTheme
import kotlinx.coroutines.launch
import javax.inject.Inject

@AndroidEntryPoint
class MainActivity : ComponentActivity() {
    @Inject lateinit var rootChecker: RootChecker
    @Inject lateinit var sessionManager: SessionManager
    @Inject lateinit var brandingStore: BrandingStore

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        val isRooted = rootChecker.check()
        OutboundSyncWorker.schedule(applicationContext)
        // Handle invite deep link when app cold-starts from the invite URL.
        handleInviteDeepLink(intent)
        // Re-post assignment from notification intent when app is cold-started from tray.
        handleAssignmentIntent(intent)
        // Refresh white-label branding for the logged-in tenant. The store emits
        // the cached brand immediately; this updates it in the background.
        if (sessionManager.isLoggedIn()) {
            lifecycleScope.launch { brandingStore.refresh() }
        }
        setContent {
            val branding by brandingStore.state.collectAsState()
            DriverAppTheme(branding = branding) {
                AppNavGraph()
                if (isRooted) {
                    var dismissed by rememberSaveable { mutableStateOf(false) }
                    if (!dismissed) {
                        AlertDialog(
                            onDismissRequest = { dismissed = true },
                            title = { Text("Security Warning") },
                            text = {
                                Text(
                                    "This device appears to be rooted. " +
                                    "Using the driver app on a rooted device may violate company policy " +
                                    "and could expose sensitive delivery data. " +
                                    "Please contact your supervisor."
                                )
                            },
                            confirmButton = {
                                TextButton(onClick = { dismissed = true }) {
                                    Text("I Understand")
                                }
                            }
                        )
                    }
                }
            }
        }
    }

    // Handle notification tap or deep link when app is re-opened or brought to foreground.
    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        handleInviteDeepLink(intent)
        handleAssignmentIntent(intent)
    }

    /**
     * Handles invite deep links of the form:
     *   https://driver.cargomarket.net/join?t=<slug>&p=<phone>&sig=<hmac>
     *
     * Extracts the three params and stores them in SessionManager so the
     * PhoneScreen can pre-populate phone + tenant and skip company-code entry.
     */
    private fun handleInviteDeepLink(intent: Intent?) {
        val uri: Uri = intent?.data ?: return
        if (uri.host != "driver.cargomarket.net" || uri.path != "/join") return
        val slug  = uri.getQueryParameter("t") ?: return
        val phone = uri.getQueryParameter("p") ?: return
        val sig   = uri.getQueryParameter("sig") ?: return
        sessionManager.savePendingInvite(slug, phone, sig)
        android.util.Log.d("MainActivity", "Invite deep link received: tenant=$slug phone=$phone")
    }

    /**
     * When the driver taps a `task_assigned` / `task_offer` system notification,
     * the PendingIntent carries all FCM data fields as extras. If
     * PendingAssignmentBus is empty (process was killed), re-post from the
     * intent — ShiftScaffold navigates to AssignmentScreen for 1:1 assignments;
     * grab offers render on the Home card instead.
     */
    private fun handleAssignmentIntent(intent: Intent?) {
        val type = intent?.getStringExtra("notification_type")
        if (type != "task_assigned" && type != "task_offer") return
        if (PendingAssignmentBus.pending.value != null) return  // already queued by onMessageReceived

        val assignmentId = intent.getStringExtra("assignment_id") ?: ""
        val offerId      = intent.getStringExtra("offer_id") ?: ""
        // Each flow needs its own id to be actionable.
        if (type == "task_assigned" && assignmentId.isBlank()) return
        if (type == "task_offer" && offerId.isBlank()) return

        PendingAssignmentBus.post(
            AssignmentPayload(
                assignmentId   = assignmentId,
                shipmentId     = intent.getStringExtra("shipment_id")     ?: "",
                customerName   = intent.getStringExtra("customer_name")   ?: "Unknown Customer",
                address        = intent.getStringExtra("address")         ?: "",
                taskType       = intent.getStringExtra("task_type")       ?: "delivery",
                trackingNumber = intent.getStringExtra("tracking_number") ?: "",
                codAmountCents = intent.getStringExtra("cod_amount_cents")?.toLongOrNull() ?: 0L,
                merchantName     = intent.getStringExtra("merchant_name")     ?: "",
                deliveryCategory = intent.getStringExtra("delivery_category") ?: "parcel",
                weightGrams      = intent.getStringExtra("weight_grams")?.toLongOrNull() ?: 0L,
                pickupLat        = intent.getStringExtra("pickup_lat")?.toDoubleOrNull(),
                pickupLng        = intent.getStringExtra("pickup_lng")?.toDoubleOrNull(),
                deliveryLat      = intent.getStringExtra("delivery_lat")?.toDoubleOrNull(),
                deliveryLng      = intent.getStringExtra("delivery_lng")?.toDoubleOrNull(),
                offerId          = offerId,
                payoutCents      = intent.getStringExtra("payout_cents")?.toLongOrNull(),
                expiresAtMillis  = intent.getStringExtra("expires_at_ms")?.toLongOrNull(),
            )
        )
    }
}
