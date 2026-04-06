package io.logisticos.driver.core.network.service

import io.logisticos.driver.core.network.model.RefreshRequest
import io.logisticos.driver.core.network.model.TokenResponse
import retrofit2.http.Body
import retrofit2.http.POST

interface IdentityApiService {
    @POST("v1/auth/refresh")
    suspend fun refreshToken(@Body request: RefreshRequest): TokenResponse
}
