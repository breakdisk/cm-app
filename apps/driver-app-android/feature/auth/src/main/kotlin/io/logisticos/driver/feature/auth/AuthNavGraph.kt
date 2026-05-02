package io.logisticos.driver.feature.auth

import androidx.navigation.NavGraphBuilder
import androidx.navigation.NavHostController
import androidx.navigation.compose.composable
import androidx.navigation.navigation
import io.logisticos.driver.feature.auth.ui.OtpScreen
import io.logisticos.driver.feature.auth.ui.PhoneScreen

const val AUTH_GRAPH = "auth_graph"
const val PHONE_ROUTE = "phone"
const val OTP_ROUTE = "otp/{identifier}"

fun NavGraphBuilder.authNavGraph(
    navController: NavHostController,
    onAuthenticated: () -> Unit
) {
    navigation(startDestination = PHONE_ROUTE, route = AUTH_GRAPH) {
        composable(PHONE_ROUTE) {
            PhoneScreen(onOtpSent = { identifier ->
                navController.navigate("otp/$identifier")
            })
        }
        composable(OTP_ROUTE) { backStack ->
            val identifier = backStack.arguments?.getString("identifier") ?: ""
            OtpScreen(identifier = identifier, onAuthenticated = onAuthenticated)
        }
    }
}
