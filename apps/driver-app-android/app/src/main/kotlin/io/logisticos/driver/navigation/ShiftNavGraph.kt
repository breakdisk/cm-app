package io.logisticos.driver.navigation

import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Scaffold
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.navigation.NavGraphBuilder
import androidx.navigation.NavHostController
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.rememberNavController
import androidx.navigation.navigation
import io.logisticos.driver.core.database.entity.TaskType
import io.logisticos.driver.feature.delivery.ui.ArrivalScreen
import io.logisticos.driver.feature.home.ui.HomeScreen
import io.logisticos.driver.feature.navigation.ui.NavigationScreen
import io.logisticos.driver.feature.notifications.presentation.NotificationsViewModel
import io.logisticos.driver.feature.notifications.ui.NotificationsScreen
import io.logisticos.driver.feature.pickup.ui.PickupScreen
import io.logisticos.driver.feature.pod.ui.PodScreen
import io.logisticos.driver.feature.profile.presentation.ProfileViewModel
import io.logisticos.driver.feature.profile.ui.ProfileScreen
import io.logisticos.driver.feature.route.ui.RouteScreen
import io.logisticos.driver.feature.scanner.ui.ScannerScreen

// ── Route constants ───────────────────────────────────────────────────────────
private const val HOME_ROUTE          = "home"
private const val ROUTE_ROUTE         = "route"
private const val SCAN_ROUTE          = "scan"
private const val NOTIFICATIONS_ROUTE = "notifications"
private const val PROFILE_ROUTE       = "profile"
private const val NAVIGATE_TO_STOP_ROUTE = "navigate/{taskId}"
private const val ARRIVAL_ROUTE       = "arrival/{taskId}"
private const val PICKUP_ROUTE        = "pickup/{taskId}"
// isCod and codAmount forwarded as string args to avoid ViewModel duplication
private const val POD_ROUTE =
    "pod/{taskId}/{requiresPhoto}/{requiresSignature}/{requiresOtp}/{isCod}/{codAmount}"

/**
 * Top-level shift scaffold: owns the BottomNavBar and an inner NavHost.
 * Manages the 5 bottom-tab destinations + deep task destinations:
 *   NavigationScreen → ArrivalScreen → PodScreen (delivery)
 *   NavigationScreen → ArrivalScreen → PickupScreen (pickup)
 */
@Composable
fun ShiftScaffold(rootNavController: NavHostController) {
    val shiftNavController = rememberNavController()

    val notifVm: NotificationsViewModel = hiltViewModel()
    val unreadCount by notifVm.unreadCount.collectAsState()

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
                    onLogout = {
                        vm.sessionManager.clearSession()
                        rootNavController.navigate(io.logisticos.driver.feature.auth.AUTH_GRAPH) {
                            popUpTo(SHIFT_GRAPH) { inclusive = true }
                        }
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
                        // Pickup tasks → PickupScreen (parcel-collection confirmation flow).
                        // Delivery / Return / Hub-drop → PodScreen (capture photo/sig/OTP/COD).
                        // Without this branch, pickups landed on PodScreen with all
                        // requires-* false and the driver saw an empty capture screen.
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
                    taskId = taskId,
                    requiresPhoto = photo,
                    requiresSignature = sig,
                    requiresOtp = otp,
                    isCod = isCod,
                    codAmount = codAmount,
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
