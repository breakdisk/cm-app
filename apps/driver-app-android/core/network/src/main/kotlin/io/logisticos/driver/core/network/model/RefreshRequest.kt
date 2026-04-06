package io.logisticos.driver.core.network.model

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable
data class RefreshRequest(
    @SerialName("refresh_token") val refreshToken: String
)

@Serializable
data class TokenResponse(
    val jwt: String,
    @SerialName("refresh_token") val refreshToken: String
)
