package io.logisticos.driver.core.network.auth

import android.content.Context
import android.content.SharedPreferences
import androidx.security.crypto.EncryptedSharedPreferences
import androidx.security.crypto.MasterKey
import dagger.hilt.android.qualifiers.ApplicationContext
import javax.inject.Inject
import javax.inject.Singleton

private const val PREFS_FILE = "logisticos_secure_prefs"

@Singleton
class EncryptedTokenStorage @Inject constructor(
    @ApplicationContext private val context: Context
) : TokenStorage {

    private val prefs by lazy {
        try {
            openPrefs()
        } catch (_: Exception) {
            // Keystore key was rotated (e.g. APK reinstall). Wipe stale ciphertext and
            // start fresh — the user will be taken to the login screen automatically.
            context.deleteSharedPreferences(PREFS_FILE)
            openPrefs()
        }
    }

    private fun openPrefs(): SharedPreferences {
        val masterKey = MasterKey.Builder(context)
            .setKeyScheme(MasterKey.KeyScheme.AES256_GCM)
            .build()
        return EncryptedSharedPreferences.create(
            context,
            PREFS_FILE,
            masterKey,
            EncryptedSharedPreferences.PrefKeyEncryptionScheme.AES256_SIV,
            EncryptedSharedPreferences.PrefValueEncryptionScheme.AES256_GCM
        )
    }

    override fun saveJwt(token: String) = prefs.edit().putString(KEY_JWT, token).apply()
    override fun getJwt(): String? = prefs.getString(KEY_JWT, null)
    override fun saveRefreshToken(token: String) = prefs.edit().putString(KEY_REFRESH, token).apply()
    override fun getRefreshToken(): String? = prefs.getString(KEY_REFRESH, null)
    override fun saveTenantId(tenantId: String) = prefs.edit().putString(KEY_TENANT, tenantId).apply()
    override fun getTenantId(): String? = prefs.getString(KEY_TENANT, null)
    override fun saveDriverId(driverId: String) = prefs.edit().putString(KEY_DRIVER, driverId).apply()
    override fun getDriverId(): String? = prefs.getString(KEY_DRIVER, null)
    override fun clearAll() = prefs.edit().clear().apply()

    companion object {
        private const val KEY_JWT = "jwt"
        private const val KEY_REFRESH = "refresh_token"
        private const val KEY_TENANT = "tenant_id"
        private const val KEY_DRIVER = "driver_id"
    }
}
