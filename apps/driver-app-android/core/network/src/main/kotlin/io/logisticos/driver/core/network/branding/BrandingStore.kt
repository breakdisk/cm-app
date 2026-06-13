package io.logisticos.driver.core.network.branding

import android.content.Context
import dagger.hilt.android.qualifiers.ApplicationContext
import io.logisticos.driver.core.network.service.IdentityApiService
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import javax.inject.Inject
import javax.inject.Singleton

/**
 * White-label brand state the UI themes against. Colour fields are nullable hex
 * strings ("#00E5FF"); null means "use the built-in default" so a non-white-label
 * tenant renders the standard neon palette.
 */
data class BrandingState(
    val displayName: String = "LogisticOS",
    val logoUrl: String? = null,
    val primaryHex: String? = null,
    val secondaryHex: String? = null,
    val accentHex: String? = null,
)

/**
 * Single source of truth for the driver app's white-label branding.
 *
 * Offline-first: the last-known brand is persisted in plain SharedPreferences
 * (branding is non-sensitive) and emitted immediately on cold start, before any
 * network call. [refresh] pulls the authenticated tenant's brand and updates the
 * flow + cache; failures keep the cached brand. Mirrors how [SessionManager]
 * caches identity for hot-path reads.
 */
@Singleton
class BrandingStore @Inject constructor(
    @ApplicationContext context: Context,
    private val identityApi: IdentityApiService,
) {
    private val prefs = context.getSharedPreferences(PREFS_FILE, Context.MODE_PRIVATE)

    private val _state = MutableStateFlow(load())
    val state: StateFlow<BrandingState> = _state.asStateFlow()

    private fun load(): BrandingState = BrandingState(
        displayName  = prefs.getString(KEY_NAME, "LogisticOS") ?: "LogisticOS",
        logoUrl      = prefs.getString(KEY_LOGO, null),
        primaryHex   = prefs.getString(KEY_PRIMARY, null),
        secondaryHex = prefs.getString(KEY_SECONDARY, null),
        accentHex    = prefs.getString(KEY_ACCENT, null),
    )

    /** Fetch the tenant brand and update the flow + cache. No-op on failure. */
    suspend fun refresh() {
        try {
            val b = identityApi.getBranding().data
            val next = BrandingState(
                displayName  = b.displayName,
                logoUrl      = b.logoUrl,
                primaryHex   = b.primaryColor,
                secondaryHex = b.secondaryColor,
                accentHex    = b.accentColor,
            )
            _state.value = next
            prefs.edit()
                .putString(KEY_NAME, next.displayName)
                .putString(KEY_LOGO, next.logoUrl)
                .putString(KEY_PRIMARY, next.primaryHex)
                .putString(KEY_SECONDARY, next.secondaryHex)
                .putString(KEY_ACCENT, next.accentHex)
                .apply()
        } catch (_: Exception) {
            // Keep the cached brand — offline or transient failure is non-fatal.
        }
    }

    /** Reset to the default brand on logout. */
    fun clear() {
        prefs.edit().clear().apply()
        _state.value = BrandingState()
    }

    companion object {
        private const val PREFS_FILE    = "logisticos_branding"
        private const val KEY_NAME      = "display_name"
        private const val KEY_LOGO      = "logo_url"
        private const val KEY_PRIMARY   = "primary_color"
        private const val KEY_SECONDARY = "secondary_color"
        private const val KEY_ACCENT    = "accent_color"
    }
}
