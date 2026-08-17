package net.cargomarket.omnideliv.courier

import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.ViewModel
import androidx.navigation.NavType
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.rememberNavController
import androidx.navigation.navArgument
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.flow.StateFlow
import net.cargomarket.omnideliv.courier.data.TokenStore
import net.cargomarket.omnideliv.courier.ui.EarningsScreen
import net.cargomarket.omnideliv.courier.ui.ManifestRoute
import net.cargomarket.omnideliv.courier.ui.ShiftScreen
import net.cargomarket.omnideliv.courier.ui.SignInScreen
import javax.inject.Inject

/**
 * Seven screens, no more: sign-in, shift, manifest, stop detail, proof capture,
 * delivered, earnings.
 *
 * Wired: sign-in, shift, manifest. Stop detail is folded into the manifest's
 * focus card rather than being its own destination — a separate screen would put
 * a navigation between a courier and the button they came to press. Proof
 * capture and earnings are not built; their domain rules are (`ProofEncoding`,
 * `Earnings`), so what is missing is screens, not logic.
 */
object Routes {
    const val SHIFT = "shift"
    const val EARNINGS = "earnings"
    /**
     * Carries both ids on purpose.
     *
     * The manifest is read by *order*, but every milestone is reported against
     * the *assignment* — two different identifiers owned by two different
     * services. Navigating with only the order id loses the one the money path
     * needs, and there is no way to recover it from the manifest.
     */
    const val MANIFEST = "manifest/{orderId}/{assignmentId}"

    fun manifest(orderId: String, assignmentId: String) = "manifest/$orderId/$assignmentId"
}

const val ARG_ORDER_ID = "orderId"
const val ARG_ASSIGNMENT_ID = "assignmentId"

/**
 * Exposes the session as observable state.
 *
 * A ViewModel rather than reading [TokenStore] straight from the composable, so
 * the gate survives configuration change and cannot be given a token that was
 * captured once at construction.
 */
@HiltViewModel
class SessionViewModel @Inject constructor(tokens: TokenStore) : ViewModel() {
    val token: StateFlow<String?> = tokens.token
}

/**
 * The session gate.
 *
 * **This collects a flow, and that is the whole point.** A previous app in this
 * project gated on a token read once when the composable mounted: a correct OTP
 * wrote the token, nothing recomposed, and the courier was thrown back to
 * sign-in until they force-quit. Collecting means the write that stores the
 * token is the same event that opens the gate, so the two cannot disagree.
 */
@Composable
fun CourierNavHost(session: SessionViewModel = hiltViewModel()) {
    val token by session.token.collectAsState()

    if (token == null) {
        // Deliberately not a NavHost destination. Sign-in is not somewhere a
        // courier navigates to or can back-stack out of — it is the absence of a
        // session, and modelling it as a route invites a back gesture that lands
        // on an authenticated screen with no token behind it.
        SignInScreen()
        return
    }

    val nav = rememberNavController()
    NavHost(navController = nav, startDestination = Routes.SHIFT) {
        composable(Routes.EARNINGS) { EarningsScreen() }

        composable(Routes.SHIFT) {
            ShiftScreen(
                onEarnings = { nav.navigate(Routes.EARNINGS) },
                onClaimed = { orderId, assignmentId ->
                    nav.navigate(Routes.manifest(orderId, assignmentId)) {
                        // The shift screen is not somewhere to go back to while
                        // holding a job: field-ops permits one live claim, so an
                        // offer list behind a claimed job can only offer work the
                        // courier is forbidden to take.
                        popUpTo(Routes.SHIFT) { inclusive = true }
                    }
                },
            )
        }

        composable(
            route = Routes.MANIFEST,
            arguments = listOf(
                navArgument(ARG_ORDER_ID) { type = NavType.StringType },
                navArgument(ARG_ASSIGNMENT_ID) { type = NavType.StringType },
            ),
        ) { entry ->
            // Read here rather than inside the screen so the screen stays a pure
            // function of its arguments and can be previewed without navigation.
            ManifestRoute(
                orderId = entry.arguments?.getString(ARG_ORDER_ID).orEmpty(),
                assignmentId = entry.arguments?.getString(ARG_ASSIGNMENT_ID).orEmpty(),
            )
        }
    }
}
