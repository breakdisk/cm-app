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
    fun getDriverId(): String? = tokenStorage.getDriverId()
    fun saveDriverId(driverId: String) = tokenStorage.saveDriverId(driverId)

    /** Runtime tenant slug — resolved from invite link or company code, persisted after login. */
    fun getTenantSlug(): String? = tokenStorage.getTenantSlug()
    fun saveTenantSlug(slug: String) = tokenStorage.saveTenantSlug(slug)

    /**
     * Stores the invite payload after the driver taps the deep link.
     * MainActivity calls this; PhoneScreen reads it once and calls clearPendingInvite().
     */
    fun savePendingInvite(slug: String, phone: String, sig: String) =
        tokenStorage.savePendingInvite(slug, phone, sig)

    /** Returns Triple(slug, phone, sig) or null if no deep link was tapped. */
    fun getPendingInvite(): Triple<String, String, String>? = tokenStorage.getPendingInvite()

    fun clearPendingInvite() = tokenStorage.clearPendingInvite()

    // Hub scanner profile — populated after OTP login and refreshed on foreground.
    fun getHubId(): String? = tokenStorage.getHubId()
    fun saveHubId(hubId: String?) = tokenStorage.saveHubId(hubId)
    fun isHubScanner(): Boolean = tokenStorage.isHubScanner()
    fun saveIsHubScanner(isHub: Boolean) = tokenStorage.saveIsHubScanner(isHub)

    fun clearSession() {
        tokenStorage.clearAll()
        cachedJwt = null
    }
}
