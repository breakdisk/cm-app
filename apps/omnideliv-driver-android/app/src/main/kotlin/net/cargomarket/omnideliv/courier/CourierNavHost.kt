package net.cargomarket.omnideliv.courier

import androidx.compose.runtime.Composable
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.rememberNavController
import net.cargomarket.omnideliv.courier.ui.ManifestScreen

/**
 * Seven screens, no more: sign-in, shift, manifest, stop detail, proof capture,
 * delivered, earnings.
 *
 * Only the manifest is wired so far — it is the screen the app lives on, and
 * the one whose layout decision (focus card plus rail) the rest depends on.
 */
object Routes {
    const val MANIFEST = "manifest"
}

@Composable
fun CourierNavHost() {
    val nav = rememberNavController()
    NavHost(navController = nav, startDestination = Routes.MANIFEST) {
        composable(Routes.MANIFEST) { ManifestScreen() }
    }
}
