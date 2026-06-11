package io.logisticos.driver.navigation

import android.app.Activity
import android.content.Intent
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Scaffold
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.navigation.NavGraphBuilder
import androidx.navigation.NavHostController
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.rememberNavController
import androidx.navigation.navigation
import io.logisticos.driver.core.common.PendingAssignmentBus
import io.logisticos.driver.core.database.entity.TaskType
import androidx.navigation.NavType
import androidx.navigation.navArgument
import io.logisticos.driver.feature.assignment.ui.AssignmentScreen
import io.logisticos.driver.feature.boxmeasure.presentation.BoxMeasureMode
import io.logisticos.driver.feature.boxmeasure.ui.BookShipmentScreen
import io.logisticos.driver.feature.boxmeasure.ui.BoxMeasureScreen
import io.logisticos.driver.feature.delivery.ui.ArrivalScreen
import io.logisticos.driver.feature.home.ui.HomeScreen
import io.logisticos.driver.feature.home.ui.HubScreen
import io.logisticos.driver.feature.navigation.ui.NavigationScreen
import io.logisticos.driver.feature.notifications.presentation.NotificationsViewModel
import io.logisticos.driver.feature.notifications.ui.NotificationsScreen
import io.logisticos.driver.feature.pickup.ui.PickupScreen
import io.logisticos.driver.feature.pod.ui.PodScreen
import io.logisticos.driver.feature.profile.presentation.ProfileViewModel
import io.logisticos.driver.feature.profile.ui.ComplianceScreen
import io.logisticos.driver.feature.profile.ui.ProfileScreen
import io.logisticos.driver.feature.route.presentation.RouteViewModel
import io.logisticos.driver.feature.route.ui.RouteScreen
import io.logisticos.driver.core.location.LocationForegroundService
import io.logisticos.driver.feature.scanner.ui.ScannerScreen

// ── Route constants ───────────────────────────────────────────────────────────
private const val HOME_ROUTE             = "home"
private const val HUB_ROUTE              = "hub"
private const val ROUTE_ROUTE            = "route"
private const val SCAN_ROUTE             = "scan"
private const val NOTIFICATIONS_ROUTE    = "notifications"
private const val PROFILE_ROUTE          = "profile"
private const val COMPLIANCE_ROUTE       = "compliance"
private const val NAVIGATE_TO_STOP_ROUTE = "navigate/{taskId}"
private const val ARRIVAL_ROUTE          = "arrival/{taskId}"
private const val PICKUP_ROUTE           = "pickup/{taskId}"
/** Assignment screen uses saved state (pendingPayload) rather than nav args. */
private const val ASSIGNMENT_ROUTE       = "assignment"
// isCod and codAmount forwarded as string args to avoid ViewModel duplication
private const val POD_ROUTE =
    "pod/{taskId}/{requiresPhoto}/{requiresSignature}/{requiresOtp}/{isCod}/{codAmount}"
// Box measure: mode=verify|quote; declared dims are optional query params
private const val BOX_MEASURE_ROUTE =
    "box_measure/{mode}?sizeId={sizeId}&declL={declL}&declW={declW}&declH={declH}"
// Book-shipment: all params are passed as URL-encoded query strings so the
// BookShipmentScreen can display a pre-filled form without ViewModel coupling.
private const val BOOK_SHIPMENT_ROUTE =
    "book_shipment?sizeId={sizeId}&mode={mode}&origin={origin}&l={l}&w={w}&h={h}&total={total}&currency={currency}"

/**
 * Top-level shift scaffold: owns the BottomNavBar and an inner NavHost.
 * Also observes [PendingAssignmentBus] — when a `task_assigned` FCM arrives,
 * navigates to [AssignmentScreen] immediately regardless of current tab.
 */
@Composable
fun ShiftScaffold(rootNavController: NavHostController) {
    val shiftNavController = rememberNavController()

    val notifVm: NotificationsViewModel = hiltViewModel()
    val unreadCount by notifVm.unreadCount.collectAsState()

    // ── FCM deeplink: task_assigned → AssignmentScreen ───────────────────────
    // Collect from PendingAssignmentBus.pending (StateFlow<AssignmentPayload?>).
    // StateFlow replays its current value on collection, so a cold-start FCM tap is
    // never lost. Navigating to AssignmentScreen only when payload transitions to
    // non-null prevents re-navigation on recomposition when payload is already null.
    val pendingPayload by PendingAssignmentBus.pending.collectAsState()

    LaunchedEffect(pendingPayload) {
        if (pendingPayload != null) {
            shiftNavController.navigate(ASSIGNMENT_ROUTE) {
                // Don't stack multiple assignment screens if the driver is slow to respond.
                launchSingleTop = true
            }
        }
    }

    Scaffold(
        containerColor = NavCanvas,
        bottomBar = {
            BottomNavBar(navController = shiftNavController, unreadCount = unreadCount)
        }
    ) { innerPadding ->
        NavHost(
            navController = shiftNavController,
            startDestination = HOME_ROUTE,
            modifier = Modifier.padding(innerPadding)
        ) {

            // ── Bottom tab destinations ───────────────────────────────────
            composable(HOME_ROUTE) {
                HomeScreen(
                    onNavigateToRoute = { shiftNavController.navigate(ROUTE_ROUTE) },
                    onNavigateToCompliance = { shiftNavController.navigate(COMPLIANCE_ROUTE) },
                    onNavigateToBoxMeasure = { shiftNavController.navigate("box_measure/quote") },
                    onNavigateToHub = { shiftNavController.navigate(HUB_ROUTE) },
                )
            }

            composable(HUB_ROUTE) {
                // Tapping a hub task hands off to the same arrival flow used by
                // the route screen; that screen already routes HUB_DROP / RETURN
                // through PickupScreen for the custody-open confirmation.
                HubScreen(
                    onSelectTask = { taskId ->
                        shiftNavController.navigate("navigate/$taskId")
                    },
                    onBack = { shiftNavController.popBackStack() },
                )
            }

            composable(ROUTE_ROUTE) {
                RouteScreen(
                    shiftId = "",
                    onNavigateToStop = { taskId ->
                        shiftNavController.navigate("navigate/$taskId")
                    },
                )
            }

            composable(SCAN_ROUTE) {
                // RouteViewModel.Factory with shiftId="" reactively resolves the active
                // shift from Room — the AWB list updates in real-time as syncs complete.
                // Previously passing emptyList() made the scanner always show 0/0.
                val routeVm: RouteViewModel = hiltViewModel<RouteViewModel, RouteViewModel.Factory>(
                    creationCallback = { factory -> factory.create("") }
                )
                val routeState by routeVm.uiState.collectAsState()
                ScannerScreen(
                    expectedAwbs = routeState.activeTasks.map { it.awb },
                    onAllScanned = { shiftNavController.popBackStack() }
                )
            }

            composable(NOTIFICATIONS_ROUTE) {
                NotificationsScreen(viewModel = hiltViewModel())
            }

            composable(PROFILE_ROUTE) {
                val vm: ProfileViewModel = hiltViewModel()
                val context = LocalContext.current
                ProfileScreen(
                    onNavigateToCompliance = {
                        shiftNavController.navigate(COMPLIANCE_ROUTE)
                    },
                    onLogout = {
                        // 1. Cancel all in-flight OkHttp calls + clear session tokens.
                        //    This unblocks any TokenAuthenticator.runBlocking threads that
                        //    are waiting on a refresh call, freeing the OkHttp dispatcher
                        //    slots before the login screen's sendOtp call is enqueued.
                        vm.logout()
                        // 2. Stop the location foreground service so it doesn't continue
                        //    publishing GPS fixes (or restart via START_STICKY) while the
                        //    driver is logged out.
                        context.stopService(Intent(context, LocationForegroundService::class.java))
                        // 3. Restart the Activity with CLEAR_TASK so the NavController's
                        //    saved back-stack Bundle is discarded. recreate() preserves the
                        //    Bundle (treated as a rotation) and the NavHost would restore to
                        //    shift_scaffold instead of AUTH_GRAPH, leaving all API calls
                        //    unauthenticated and causing infinite spinners.
                        val intent = Intent(context, io.logisticos.driver.MainActivity::class.java).apply {
                            flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TASK
                        }
                        context.startActivity(intent)
                        (context as? Activity)?.finish()
                    },
                    viewModel = vm
                )
            }

            composable(COMPLIANCE_ROUTE) {
                ComplianceScreen(onBack = { shiftNavController.popBackStack() })
            }

            // ── Assignment accept/reject ──────────────────────────────────
            composable(ASSIGNMENT_ROUTE) {
                val payload = pendingPayload
                if (payload == null) {
                    // Stale nav entry (e.g. back-stack restoration after process death) —
                    // pop back rather than showing an empty screen.
                    LaunchedEffect(Unit) { shiftNavController.popBackStack() }
                    return@composable
                }
                AssignmentScreen(
                    payload    = payload,
                    onAccepted = {
                        PendingAssignmentBus.clear()
                        shiftNavController.navigate(ROUTE_ROUTE) {
                            popUpTo(HOME_ROUTE)
                        }
                    },
                    onRejected = {
                        PendingAssignmentBus.clear()
                        shiftNavController.popBackStack()
                    }
                )
            }

            // ── Deep task destinations ────────────────────────────────────

            composable(NAVIGATE_TO_STOP_ROUTE) { backStack ->
                val taskId = backStack.arguments?.getString("taskId") ?: ""
                NavigationScreen(
                    taskId = taskId,
                    onArrived = {
                        shiftNavController.navigate(ARRIVAL_ROUTE.replace("{taskId}", taskId))
                    },
                    onBack = { shiftNavController.popBackStack() }
                )
            }

            composable(ARRIVAL_ROUTE) { backStack ->
                val taskId = backStack.arguments?.getString("taskId") ?: ""
                ArrivalScreen(
                    taskId = taskId,
                    onStartTask = { id, taskType, photo, sig, otp, isCod, codAmount ->
                        when (taskType) {
                            // PICKUP: driver collects from merchant (first-mile).
                            // RETURN / HUB_DROP: driver returns undelivered parcel to hub —
                            // same custody-open flow as pickup (scan AWB, optional photo, confirm).
                            // These must NOT go to PodScreen which expects a customer signature.
                            TaskType.PICKUP, TaskType.RETURN, TaskType.HUB_DROP ->
                                shiftNavController.navigate(PICKUP_ROUTE.replace("{taskId}", id))
                            else -> shiftNavController.navigate(
                                "pod/$id/$photo/$sig/$otp/$isCod/$codAmount"
                            )
                        }
                    },
                    onBack = { shiftNavController.popBackStack() },
                )
            }

            composable(PICKUP_ROUTE) { backStack ->
                val taskId = backStack.arguments?.getString("taskId") ?: ""
                PickupScreen(
                    taskId = taskId,
                    onCompleted = {
                        shiftNavController.navigate(HOME_ROUTE) {
                            popUpTo(HOME_ROUTE) { inclusive = true }
                        }
                    },
                    onBack = { shiftNavController.popBackStack() },
                    onNavigateToBoxMeasure = { sizeId, declL, declW, declH ->
                        val query = listOfNotNull(
                            sizeId?.let { "sizeId=$it" },
                            declL?.let { "declL=$it" },
                            declW?.let { "declW=$it" },
                            declH?.let { "declH=$it" },
                        ).joinToString("&")
                        val route = if (query.isEmpty()) "box_measure/verify" else "box_measure/verify?$query"
                        shiftNavController.navigate(route)
                    },
                )
            }

            // ── Box measurement / quote ────────────────────────────────────
            composable(
                route = BOX_MEASURE_ROUTE,
                arguments = listOf(
                    navArgument("mode") { type = NavType.StringType },
                    navArgument("sizeId") { type = NavType.StringType; nullable = true; defaultValue = null },
                    navArgument("declL")  { type = NavType.StringType; nullable = true; defaultValue = null },
                    navArgument("declW")  { type = NavType.StringType; nullable = true; defaultValue = null },
                    navArgument("declH")  { type = NavType.StringType; nullable = true; defaultValue = null },
                )
            ) { backStack ->
                val args    = backStack.arguments
                val modeStr = args?.getString("mode") ?: "quote"
                val mode    = if (modeStr == "verify") BoxMeasureMode.VERIFY else BoxMeasureMode.QUOTE
                val sizeId  = args?.getString("sizeId")
                val declL   = args?.getString("declL")?.toDoubleOrNull()
                val declW   = args?.getString("declW")?.toDoubleOrNull()
                val declH   = args?.getString("declH")?.toDoubleOrNull()
                BoxMeasureScreen(
                    mode               = mode,
                    declaredSizeId     = sizeId,
                    declaredL          = declL,
                    declaredW          = declW,
                    declaredH          = declH,
                    // Store verified dims in the PickupScreen's back-stack entry so
                    // PickupViewModel.confirmPickup() can include them in the POP payload
                    // (Track-A Balikbayan: verified_box_size, outer_packaging_integrity).
                    // BoxMeasureScreen calls onBack() immediately after this callback —
                    // do NOT also call popBackStack() here or the back stack pops twice.
                    onDimensionsVerified = { l: Double, w: Double, h: Double ->
                        shiftNavController.previousBackStackEntry?.savedStateHandle?.run {
                            set("verified_box_l", l.toFloat())
                            set("verified_box_w", w.toFloat())
                            set("verified_box_h", h.toFloat())
                        }
                    },
                    // QUOTE mode only: driver taps "Book This Shipment" after getting a price.
                    // Navigate to BookShipmentScreen with all quote data as route params so the
                    // form is pre-filled and the driver just adds customer details to complete.
                    onBookShipment = { bSizeId, bMode, bOrigin, bL, bW, bH, bTotal, bCurrency ->
                        val route = "book_shipment" +
                            "?sizeId=${bSizeId}" +
                            "&mode=${bMode.name}" +
                            "&origin=${java.net.URLEncoder.encode(bOrigin, "UTF-8")}" +
                            "&l=$bL&w=$bW&h=$bH" +
                            "&total=$bTotal" +
                            "&currency=${bCurrency}"
                        shiftNavController.navigate(route)
                    },
                    onBack             = { shiftNavController.popBackStack() },
                )
            }

            // ── Book shipment (from QUOTE mode quote result) ──────────────────
            composable(
                route = BOOK_SHIPMENT_ROUTE,
                arguments = listOf(
                    navArgument("sizeId")   { type = NavType.StringType; nullable = true; defaultValue = null },
                    navArgument("mode")     { type = NavType.StringType; nullable = true; defaultValue = null },
                    navArgument("origin")   { type = NavType.StringType; nullable = true; defaultValue = null },
                    navArgument("l")        { type = NavType.StringType; nullable = true; defaultValue = null },
                    navArgument("w")        { type = NavType.StringType; nullable = true; defaultValue = null },
                    navArgument("h")        { type = NavType.StringType; nullable = true; defaultValue = null },
                    navArgument("total")    { type = NavType.StringType; nullable = true; defaultValue = null },
                    navArgument("currency") { type = NavType.StringType; nullable = true; defaultValue = null },
                )
            ) { backStack ->
                val args = backStack.arguments
                BookShipmentScreen(
                    prefillSizeId    = args?.getString("sizeId") ?: "",
                    prefillFreight   = args?.getString("mode") ?: "SEA",
                    prefillOrigin    = args?.getString("origin")?.let {
                        java.net.URLDecoder.decode(it, "UTF-8") } ?: "",
                    prefillL         = args?.getString("l")?.toDoubleOrNull() ?: 0.0,
                    prefillW         = args?.getString("w")?.toDoubleOrNull() ?: 0.0,
                    prefillH         = args?.getString("h")?.toDoubleOrNull() ?: 0.0,
                    prefillTotal     = args?.getString("total")?.toDoubleOrNull() ?: 0.0,
                    prefillCurrency  = args?.getString("currency") ?: "USD",
                    onBooked         = {
                        shiftNavController.navigate(HOME_ROUTE) {
                            popUpTo(HOME_ROUTE) { inclusive = true }
                        }
                    },
                    onBack           = { shiftNavController.popBackStack() },
                )
            }

            composable(POD_ROUTE) { backStack ->
                val args      = backStack.arguments
                val taskId    = args?.getString("taskId") ?: ""
                val photo     = args?.getString("requiresPhoto") == "true"
                val sig       = args?.getString("requiresSignature") == "true"
                val otp       = args?.getString("requiresOtp") == "true"
                val isCod     = args?.getString("isCod") == "true"
                val codAmount = args?.getString("codAmount")?.toDoubleOrNull() ?: 0.0
                PodScreen(
                    taskId            = taskId,
                    requiresPhoto     = photo,
                    requiresSignature = sig,
                    requiresOtp       = otp,
                    isCod             = isCod,
                    codAmount         = codAmount,
                    onCompleted = {
                        shiftNavController.navigate(HOME_ROUTE) {
                            popUpTo(HOME_ROUTE) { inclusive = true }
                        }
                    },
                    onFailed = {
                        shiftNavController.navigate(HOME_ROUTE) {
                            popUpTo(HOME_ROUTE) { inclusive = true }
                        }
                    },
                    onBack = { shiftNavController.popBackStack() },
                )
            }
        }
    }
}

/**
 * Extension on NavGraphBuilder — registers the shift nav graph as a nested destination.
 * Called from AppNavGraph.
 */
fun NavGraphBuilder.shiftNavGraph(navController: NavHostController) {
    navigation(startDestination = "shift_scaffold", route = SHIFT_GRAPH) {
        composable("shift_scaffold") {
            ShiftScaffold(rootNavController = navController)
        }
    }
}
