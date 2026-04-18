package io.logisticos.driver.core.network.service

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import retrofit2.http.Body
import retrofit2.http.POST

@Serializable
data class ApiResponse<T>(val data: T)

@Serializable
data class OtpSendRequest(
    @SerialName("phone_number") val phone: String,
    @SerialName("tenant_slug") val tenantSlug: String? = null,
    @SerialName("role") val role: String? = "driver"
)

@Serializable
data class OtpSendResponse(val message: String)

@Serializable
data class OtpVerifyRequest(
    @SerialName("phone_number") val phone: String,
    @SerialName("otp_code") val otp: String,
    @SerialName("tenant_slug") val tenantSlug: String? = null,
    @SerialName("role") val role: String? = "driver"
)

@Serializable
data class OtpVerifyResponse(
    @SerialName("access_token") val jwt: String,
    @SerialName("refresh_token") val refreshToken: String,
    @SerialName("driver_id") val driverId: String,
    @SerialName("tenant_id") val tenantId: String
)

@Serializable
data class FcmTokenRequest(
    @SerialName("fcm_token") val fcmToken: String,
    @SerialName("driver_id") val driverId: String
)

interface IdentityApiService {
    @POST("v1/auth/otp/send")
    suspend fun sendOtp(@Body request: OtpSendRequest): ApiResponse<OtpSendResponse>

    @POST("v1/auth/otp/verify")
    suspend fun verifyOtp(@Body request: OtpVerifyRequest): ApiResponse<OtpVerifyResponse>

    @POST("v1/auth/refresh")
    suspend fun refreshToken(@Body request: io.logisticos.driver.core.network.model.RefreshRequest): ApiResponse<io.logisticos.driver.core.network.model.TokenResponse>

    @POST("v1/auth/fcm-token")
    suspend fun registerFcmToken(@Body request: FcmTokenRequest): retrofit2.Response<Unit>
}
