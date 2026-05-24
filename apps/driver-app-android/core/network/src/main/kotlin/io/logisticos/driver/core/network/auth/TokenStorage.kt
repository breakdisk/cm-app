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

    // Tenant slug — resolved at runtime from invite link or company code.
    fun saveTenantSlug(slug: String)
    fun getTenantSlug(): String?

    // Pending invite payload from a deep link tap, cleared after OTP is sent.
    fun savePendingInvite(slug: String, phone: String, sig: String)
    /** Returns Triple(slug, phone, sig) or null if no pending invite. */
    fun getPendingInvite(): Triple<String, String, String>?
    fun clearPendingInvite()

    fun clearAll()
}
