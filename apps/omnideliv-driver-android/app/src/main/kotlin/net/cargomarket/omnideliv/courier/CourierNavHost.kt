package net.cargomarket.omnideliv.courier

import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.ViewModel
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.rememberNavController
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.flow.StateFlow
import net.cargomarket.omnideliv.courier.data.TokenStore
import net.cargomarket.omnideliv.courier.ui.ManifestScreen
import net.cargomarket.omnideliv.courier.ui.SignInScreen
import javax.inject.Inject

/**
 * Seven screens, no more: sign-in, shift, manifest, stop detail, proof capture,
 * delivered, earnings.
 *
 * Sign-in and manifest are wired. Shift, proof capture and earnings are not yet
 * built — their domain rules are (`OfferCard`, `ProofEncoding`, `Earnings`), so
 * what remains is the screens rather than the logic.
 */
object Routes {
    const val MANIFEST = "manifest"
}

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
    NavHost(navController = nav, startDestination = Routes.MANIFEST) {
        composable(Routes.MANIFEST) { ManifestScreen() }
    }
}
