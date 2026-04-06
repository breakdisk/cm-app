package io.logisticos.driver.core.network.auth

import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class SessionManager @Inject constructor(
    private val tokenStorage: TokenStorage
) {
    fun isLoggedIn(): Boolean = tokenStorage.getJwt() != null

    fun isOfflineModeActive(): Boolean =
        tokenStorage.getJwt() == null && tokenStorage.getRefreshToken() != null

    fun saveTokens(jwt: String, refreshToken: String) {
        tokenStorage.saveJwt(jwt)
        tokenStorage.saveRefreshToken(refreshToken)
    }

    fun getJwt(): String? = tokenStorage.getJwt()
    fun getRefreshToken(): String? = tokenStorage.getRefreshToken()
    fun getTenantId(): String? = tokenStorage.getTenantId()
    fun saveTenantId(tenantId: String) = tokenStorage.saveTenantId(tenantId)

    fun clearSession() = tokenStorage.clearAll()
}
