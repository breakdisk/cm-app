package io.logisticos.driver.navigation

import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.navigation.NavHostController
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.rememberNavController
import io.logisticos.driver.BuildConfig
import io.logisticos.driver.MainViewModel
import io.logisticos.driver.core.designsystem.MoveTheme
import io.logisticos.driver.feature.auth.AUTH_GRAPH
import io.logisticos.driver.feature.auth.authNavGraph

const val SHIFT_GRAPH = "shift_graph"

@Composable
fun AppNavGraph() {
    val mainVm: MainViewModel = hiltViewModel()
    val startDestination = if (mainVm.isLoggedIn) SHIFT_GRAPH else AUTH_GRAPH

    val navController = rememberNavController()

    // Sun mode is one switch for the whole app — sign-in and every shift screen
    // read it from the theme. Only the Move build offers it: the staging build's
    // home screen is not on the design tokens, so it stays at night.
    var sun by rememberSaveable { mutableStateOf(false) }

    MoveTheme(
        sun = BuildConfig.MOVE_UI && sun,
        onToggleSun = if (BuildConfig.MOVE_UI) ({ sun = !sun }) else null,
    ) {
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
}

// shiftNavGraph is implemented in ShiftNavGraph.kt
