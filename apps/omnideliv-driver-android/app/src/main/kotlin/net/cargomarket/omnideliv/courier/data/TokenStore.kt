package net.cargomarket.omnideliv.courier.data

import android.content.Context
import androidx.security.crypto.EncryptedSharedPreferences
import androidx.security.crypto.MasterKey
import dagger.hilt.android.qualifiers.ApplicationContext
import net.cargomarket.omnideliv.courier.domain.SessionTokens
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import javax.inject.Inject
import javax.inject.Singleton

/**
 * Where the courier's session token lives.
 *
 * Encrypted at rest rather than plain SharedPreferences: this token authorizes
 * money-moving calls — a delivery credits a ledger — and a rooted or backed-up
 * device would otherwise hand it over in clear text.
 *
 * **The token is exposed as a [StateFlow], and that is a bug fix, not a
 * preference.** This project has already shipped a session gate that read the
 * token once when it mounted: a *correct* OTP wrote the token, nothing
 * recomposed, and the courier was bounced straight back to sign-in — recovering
 * only after a force-quit. Any auth bug that a force-quit fixes has that shape.
 * A gate that collects a flow cannot have it, because the write is what moves
 * the gate.
 */
@Singleton
class TokenStore @Inject constructor(@ApplicationContext context: Context) : SessionTokens {

    private val prefs by lazy {
        val key = MasterKey.Builder(context)
            .setKeyScheme(MasterKey.KeyScheme.AES256_GCM)
            .build()
        EncryptedSharedPreferences.create(
            context,
            "courier-session",
            key,
            EncryptedSharedPreferences.PrefKeyEncryptionScheme.AES256_SIV,
            EncryptedSharedPreferences.PrefValueEncryptionScheme.AES256_GCM,
        )
    }

    // Seeded from disk on first touch so a returning courier is already signed
    // in on the first frame and never sees the sign-in screen flash past.
    private val _token = MutableStateFlow(prefs.getString(KEY_ACCESS, null))

    /** Observable session state. Null means signed out. */
    val token: StateFlow<String?> = _token.asStateFlow()

    /**
     * The synchronous read, for the OkHttp interceptor.
     *
     * Interceptors run off the main thread and cannot suspend, so they read the
     * current value directly. Writes still go through [signIn]/[signOut], so the
     * flow can never disagree with what is on disk.
     */
    override val accessToken: String? get() = _token.value

    /**
     * Proves identity once the access token has expired.
     *
     * Stored from the moment of sign-in. Identity has always returned it; this
     * app used to drop it, which is why a shift longer than the one-hour token
     * ended in a wall of 401s.
     */
    override val refreshToken: String? get() = prefs.getString(KEY_REFRESH, null)

    /** The courier's own id, needed for the position ingest route. */
    var courierId: String?
        get() = prefs.getString(KEY_COURIER, null)
        set(value) = prefs.edit().putString(KEY_COURIER, value).apply()

    /**
     * Persist a session and move the gate, in that order.
     *
     * Disk first: if the process dies between the two, a returning courier is
     * signed in rather than signed out — the recoverable direction. The reverse
     * order would show a signed-in UI backed by nothing.
     */
    fun signIn(accessToken: String, refreshToken: String?, courierId: String?) {
        val editor = prefs.edit().putString(KEY_ACCESS, accessToken)
        if (refreshToken != null) editor.putString(KEY_REFRESH, refreshToken)
        if (courierId != null) editor.putString(KEY_COURIER, courierId)
        editor.apply()
        _token.value = accessToken
    }

    /**
     * Store a refreshed pair, keeping everything else.
     *
     * Not [signIn]: the courier id came from the OTP verify response and the
     * refresh response has no `driver_id`, so routing it through sign-in would
     * quietly erase the id every milestone call needs.
     *
     * Disk before the flow, for the same reason as [signIn].
     */
    override fun update(accessToken: String, refreshToken: String?) {
        val editor = prefs.edit().putString(KEY_ACCESS, accessToken)
        if (refreshToken != null) editor.putString(KEY_REFRESH, refreshToken)
        editor.apply()
        _token.value = accessToken
    }

    override fun signOut() {
        prefs.edit().clear().apply()
        _token.value = null
    }

    private companion object {
        const val KEY_ACCESS = "access_token"
        const val KEY_REFRESH = "refresh_token"
        const val KEY_COURIER = "courier_id"
    }
}
