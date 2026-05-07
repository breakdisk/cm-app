package io.logisticos.driver.feature.auth

import android.net.Uri
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
                // URL-encode the identifier so special characters like '+' in
                // international phone numbers (+971…) survive the navigation route
                // intact. Compose Navigation 2.8 URI-decodes path args on extraction,
                // so '+' must be encoded as '%2B' to avoid being decoded as a space.
                navController.navigate("otp/${Uri.encode(identifier)}")
            })
        }
        composable(OTP_ROUTE) { backStack ->
            val identifier = backStack.arguments?.getString("identifier") ?: ""
            OtpScreen(identifier = identifier, onAuthenticated = onAuthenticated)
        }
    }
}
