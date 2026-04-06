package io.logisticos.driver.navigation

import androidx.compose.runtime.Composable
import androidx.navigation.NavHostController
import androidx.navigation.NavGraphBuilder
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.rememberNavController
import io.logisticos.driver.feature.auth.AUTH_GRAPH
import io.logisticos.driver.feature.auth.authNavGraph

const val SHIFT_GRAPH = "shift_graph"

@Composable
fun AppNavGraph() {
    val navController = rememberNavController()

    NavHost(navController = navController, startDestination = AUTH_GRAPH) {
        authNavGraph(
            navController = navController,
            onAuthenticated = {
                navController.navigate(SHIFT_GRAPH) {
                    popUpTo(AUTH_GRAPH) { inclusive = true }
                }
            }
        )
        shiftNavGraph(navController = navController)
    }
}

// Stub for Task 22 — bottom navigation will be wired here
fun NavGraphBuilder.shiftNavGraph(navController: NavHostController) {
    // TODO: Task 22 — implement shift/home/route/delivery nav graph
}
