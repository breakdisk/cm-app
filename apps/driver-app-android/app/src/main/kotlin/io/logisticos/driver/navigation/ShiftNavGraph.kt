package io.logisticos.driver.navigation

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.navigation.NavGraphBuilder
import androidx.navigation.NavHostController
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.rememberNavController
import androidx.navigation.navigation
import io.logisticos.driver.feature.home.ui.HomeScreen
import io.logisticos.driver.feature.navigation.ui.NavigationScreen
import io.logisticos.driver.feature.notifications.presentation.NotificationsViewModel
import io.logisticos.driver.feature.notifications.ui.NotificationsScreen
import io.logisticos.driver.feature.pod.ui.PodScreen
import io.logisticos.driver.feature.profile.presentation.ProfileViewModel
import io.logisticos.driver.feature.profile.ui.ProfileScreen
import io.logisticos.driver.feature.route.ui.RouteScreen
import io.logisticos.driver.feature.scanner.ui.ScannerScreen

// Route constants used within the shift nav graph
private const val HOME_ROUTE = "home"
private const val ROUTE_ROUTE = "route"
private const val SCAN_ROUTE = "scan"
private const val NOTIFICATIONS_ROUTE = "notifications"
private const val PROFILE_ROUTE = "profile"
private const val NAVIGATE_TO_STOP_ROUTE = "navigate/{taskId}"
private const val POD_ROUTE = "pod/{taskId}/{requiresPhoto}/{requiresSignature}/{requiresOtp}"
private const val ARRIVAL_ROUTE = "arrival/{taskId}"

/**
 * Top-level shift scaffold: owns the BottomNavBar and an inner NavHost that
 * manages the five bottom-tab destinations plus deep destinations
 * (NavigationScreen, ArrivalScreen, PodScreen, ScannerScreen sub-flow).
 */
@Composable
fun ShiftScaffold(rootNavController: NavHostController) {
    val shiftNavController = rememberNavController()

    // Observe unread count from the NotificationsViewModel at scaffold level so the badge
    // updates in real-time without recomposing the full nav host.
    val notifVm: NotificationsViewModel = hiltViewModel()
    val unreadCount by notifVm.unreadCount.collectAsState()

    Scaffold(
        containerColor = NavCanvas,
        bottomBar = {
            BottomNavBar(
                navController = shiftNavController,
                unreadCount = unreadCount,
            )
        },
    ) { innerPadding ->
        NavHost(
            navController = shiftNavController,
            startDestination = HOME_ROUTE,
            modifier = Modifier.padding(innerPadding),
        ) {
            composable(HOME_ROUTE) {
                HomeScreen(
                    onNavigateToRoute = {
                        shiftNavController.navigate(ROUTE_ROUTE)
                    },
                )
            }

            composable(ROUTE_ROUTE) {
                // shiftId is not available as a nav arg in the current design; using empty
                // string causes RouteViewModel to fetch tasks across all active shifts via
                // the repository query. Replace with real shiftId once HomeViewModel exposes it.
                RouteScreen(
                    shiftId = "",
                    onNavigateToStop = { taskId ->
                        shiftNavController.navigate("navigate/$taskId")
                    },
                )
            }

            composable(SCAN_ROUTE) {
                // Scan tab: launched without task context; AWBs passed as nav args when launched from a specific task stop
                ScannerScreen(
                    expectedAwbs = emptyList(),
                    onAllScanned = {
                        shiftNavController.popBackStack()
                    },
                )
            }

            composable(NOTIFICATIONS_ROUTE) {
                val vm: NotificationsViewModel = hiltViewModel()
                NotificationsScreen(viewModel = vm)
            }

            composable(PROFILE_ROUTE) {
                val vm: ProfileViewModel = hiltViewModel()
                ProfileScreen(
                    sessionManager = vm.sessionManager,
                    isOfflineMode = vm.isOfflineMode,
                    onLogout = {
                        vm.sessionManager.clearSession()
                        // Navigate back to auth graph via the root controller
                        rootNavController.navigate(io.logisticos.driver.feature.auth.AUTH_GRAPH) {
                            popUpTo(SHIFT_GRAPH) { inclusive = true }
                        }
                    },
                )
            }

            composable(NAVIGATE_TO_STOP_ROUTE) { backStack ->
                val taskId = backStack.arguments?.getString("taskId") ?: ""
                NavigationScreen(
                    taskId = taskId,
                    onArrived = {
                        shiftNavController.navigate(ARRIVAL_ROUTE.replace("{taskId}", taskId))
                    },
                )
            }

            composable(ARRIVAL_ROUTE) { backStack ->
                val taskId = backStack.arguments?.getString("taskId") ?: ""
                // ArrivalScreen is not yet implemented — placeholder routes straight to POD.
                ArrivalPlaceholder(
                    taskId = taskId,
                    onContinueToPod = {
                        shiftNavController.navigate(
                            "pod/$taskId/true/true/false"
                        )
                    },
                )
            }

            composable(POD_ROUTE) { backStack ->
                val args = backStack.arguments
                val taskId = args?.getString("taskId") ?: ""
                val requiresPhoto = args?.getString("requiresPhoto") == "true"
                val requiresSignature = args?.getString("requiresSignature") == "true"
                val requiresOtp = args?.getString("requiresOtp") == "true"
                PodScreen(
                    taskId = taskId,
                    requiresPhoto = requiresPhoto,
                    requiresSignature = requiresSignature,
                    requiresOtp = requiresOtp,
                    onCompleted = {
                        shiftNavController.navigate(HOME_ROUTE) {
                            popUpTo(HOME_ROUTE) { inclusive = true }
                        }
                    },
                )
            }
        }
    }
}

/**
 * Inline placeholder for ArrivalScreen (Task pending implementation).
 * Shows a brief confirmation UI and routes immediately to POD capture.
 */
@Composable
private fun ArrivalPlaceholder(taskId: String, onContinueToPod: () -> Unit) {
    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(Color(0xFF050810)),
        contentAlignment = Alignment.Center,
    ) {
        androidx.compose.material3.Button(onClick = onContinueToPod) {
            Text("Arrived — Start POD Capture", color = Color(0xFF050810))
        }
    }
}

/**
 * Extension on NavGraphBuilder that registers the full shift navigation graph
 * (scaffold + bottom nav + all feature screens) as a nested navigation destination.
 * Called from [AppNavGraph].
 */
fun NavGraphBuilder.shiftNavGraph(navController: NavHostController) {
    navigation(startDestination = "shift_scaffold", route = SHIFT_GRAPH) {
        composable("shift_scaffold") {
            ShiftScaffold(rootNavController = navController)
        }
    }
}
