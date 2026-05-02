package io.logisticos.driver.navigation

import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Scaffold
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.navigation.NavGraphBuilder
import androidx.navigation.NavHostController
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.rememberNavController
import androidx.navigation.navigation
import io.logisticos.driver.core.common.PendingAssignmentBus
import io.logisticos.driver.core.database.entity.TaskType
import io.logisticos.driver.feature.assignment.ui.AssignmentScreen
import io.logisticos.driver.feature.delivery.ui.ArrivalScreen
import io.logisticos.driver.feature.home.ui.HomeScreen
import io.logisticos.driver.feature.navigation.ui.NavigationScreen
import io.logisticos.driver.feature.notifications.presentation.NotificationsViewModel
import io.logisticos.driver.feature.notifications.ui.NotificationsScreen
import io.logisticos.driver.feature.pickup.ui.PickupScreen
import io.logisticos.driver.feature.pod.ui.PodScreen
import io.logisticos.driver.feature.profile.presentation.ProfileViewModel
import io.logisticos.driver.feature.profile.ui.ComplianceScreen
import io.logisticos.driver.feature.profile.ui.ProfileScreen
import io.logisticos.driver.feature.route.ui.RouteScreen
import io.logisticos.driver.feature.scanner.ui.ScannerScreen

// ── Route constants ───────────────────────────────────────────────────────────
private const val HOME_ROUTE             = "home"
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
                HomeScreen(onNavigateToRoute = { shiftNavController.navigate(ROUTE_ROUTE) })
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
                ScannerScreen(
                    expectedAwbs = emptyList(),
                    onAllScanned = { shiftNavController.popBackStack() }
                )
            }

            composable(NOTIFICATIONS_ROUTE) {
                NotificationsScreen(viewModel = hiltViewModel())
            }

            composable(PROFILE_ROUTE) {
                val vm: ProfileViewModel = hiltViewModel()
                ProfileScreen(
                    sessionManager = vm.sessionManager,
                    isOfflineMode = vm.isOfflineMode,
                    onNavigateToCompliance = {
                        shiftNavController.navigate(COMPLIANCE_ROUTE)
                    },
                    onLogout = {
                        vm.sessionManager.clearSession()
                        rootNavController.navigate(io.logisticos.driver.feature.auth.AUTH_GRAPH) {
                            popUpTo(SHIFT_GRAPH) { inclusive = true }
                        }
                    }
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
                            TaskType.PICKUP -> shiftNavController.navigate(
                                PICKUP_ROUTE.replace("{taskId}", id)
                            )
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
