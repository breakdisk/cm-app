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
    fun clearAll()
}
