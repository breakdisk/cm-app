package io.logisticos.driver.core.network.auth

interface TokenStorage {
    fun saveJwt(token: String)
    fun getJwt(): String?
    fun saveRefreshToken(token: String)
    fun getRefreshToken(): String?
    fun saveTenantId(tenantId: String)
    fun getTenantId(): String?
    fun saveDriverId(driverId: String)
    fun getDriverId(): String?
    fun saveTenantSlug(slug: String)
    fun getTenantSlug(): String?
    fun savePendingInvite(slug: String, phone: String, sig: String)
    fun getPendingInvite(): Triple<String, String, String>?
    fun clearPendingInvite()
    // Hub scanner profile
    fun saveHubId(hubId: String?)
    fun getHubId(): String?
    fun saveIsHubScanner(isHub: Boolean)
    fun isHubScanner(): Boolean
    fun clearAll()
}
