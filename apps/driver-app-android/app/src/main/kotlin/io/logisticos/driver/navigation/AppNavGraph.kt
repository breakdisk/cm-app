package io.logisticos.driver.navigation

import androidx.compose.runtime.Composable
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.navigation.NavHostController
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.rememberNavController
import io.logisticos.driver.MainViewModel
import io.logisticos.driver.feature.auth.AUTH_GRAPH
import io.logisticos.driver.feature.auth.authNavGraph

const val SHIFT_GRAPH = "shift_graph"

@Composable
fun AppNavGraph() {
    val mainVm: MainViewModel = hiltViewModel()
    val startDestination = if (mainVm.isLoggedIn) SHIFT_GRAPH else AUTH_GRAPH

    val navController = rememberNavController()

    NavHost(navController = navController, startDestination = startDestination) {
        authNavGraph(
            navController = navController,
            onAuthenticated = {
                mainVm.onAuthSuccess()
                navController.navigate(SHIFT_GRAPH) {
                    popUpTo(AUTH_GRAPH) { inclusive = true }
                }
            }
        )
        shiftNavGraph(navController = navController)
    }
}

// shiftNavGraph is implemented in ShiftNavGraph.kt
