package net.cargomarket.omnideliv.courier.data

import android.content.Context
import androidx.security.crypto.EncryptedSharedPreferences
import androidx.security.crypto.MasterKey
import dagger.hilt.android.qualifiers.ApplicationContext
import javax.inject.Inject
import javax.inject.Singleton

/**
 * Where the courier's session token lives.
 *
 * Encrypted at rest rather than plain SharedPreferences: this token authorizes
 * money-moving calls — a delivery credits a ledger — and a rooted or backed-up
 * device would otherwise hand it over in clear text.
 */
@Singleton
class TokenStore @Inject constructor(@ApplicationContext context: Context) {

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

    var accessToken: String?
        get() = prefs.getString(KEY_ACCESS, null)
        set(value) = prefs.edit().putString(KEY_ACCESS, value).apply()

    /** The courier's own id, needed for the position ingest route. */
    var courierId: String?
        get() = prefs.getString(KEY_COURIER, null)
        set(value) = prefs.edit().putString(KEY_COURIER, value).apply()

    fun clear() = prefs.edit().clear().apply()

    private companion object {
        const val KEY_ACCESS = "access_token"
        const val KEY_COURIER = "courier_id"
    }
}
