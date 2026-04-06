package io.logisticos.driver.core.network.auth

import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class SessionManager @Inject constructor(
    private val tokenStorage: TokenStorage
) {
    // In-memory cache to avoid repeated EncryptedSharedPreferences decryption on hot paths
    @Volatile private var cachedJwt: String? = tokenStorage.getJwt()

    fun isLoggedIn(): Boolean = cachedJwt != null

    fun isOfflineModeActive(): Boolean =
        cachedJwt == null && tokenStorage.getRefreshToken() != null

    fun saveTokens(jwt: String, refreshToken: String) {
        tokenStorage.saveJwt(jwt)
        tokenStorage.saveRefreshToken(refreshToken)
        cachedJwt = jwt
    }

    fun getJwt(): String? = cachedJwt
    fun getRefreshToken(): String? = tokenStorage.getRefreshToken()
    fun getTenantId(): String? = tokenStorage.getTenantId()
    fun saveTenantId(tenantId: String) = tokenStorage.saveTenantId(tenantId)

    fun clearSession() {
        tokenStorage.clearAll()
        cachedJwt = null
    }
}
