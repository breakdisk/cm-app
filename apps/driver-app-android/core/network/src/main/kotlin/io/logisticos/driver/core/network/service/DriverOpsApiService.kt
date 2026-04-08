package io.logisticos.driver.core.network.service

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import retrofit2.http.*

@Serializable
data class ShiftResponse(
    val id: String,
    @SerialName("driver_id") val driverId: String,
    @SerialName("tenant_id") val tenantId: String,
    @SerialName("total_stops") val totalStops: Int,
    val tasks: List<TaskResponse>
)

@Serializable
data class TaskResponse(
    val id: String,
    val awb: String,
    @SerialName("recipient_name") val recipientName: String,
    @SerialName("recipient_phone") val recipientPhone: String,
    val address: String,
    val lat: Double,
    val lng: Double,
    @SerialName("stop_order") val stopOrder: Int,
    @SerialName("requires_photo") val requiresPhoto: Boolean,
    @SerialName("requires_signature") val requiresSignature: Boolean,
    @SerialName("requires_otp") val requiresOtp: Boolean,
    @SerialName("is_cod") val isCod: Boolean,
    @SerialName("cod_amount") val codAmount: Double,
    val notes: String? = null
)

@Serializable
data class TaskStatusRequest(val status: String, val reason: String? = null)

interface DriverOpsApiService {
    @GET("shifts/active")
    suspend fun getActiveShift(): ShiftResponse

    @POST("shifts/{id}/start")
    suspend fun startShift(@Path("id") shiftId: String): ShiftResponse

    @POST("shifts/{id}/end")
    suspend fun endShift(@Path("id") shiftId: String)

    @PATCH("tasks/{id}/status")
    suspend fun updateTaskStatus(@Path("id") taskId: String, @Body request: TaskStatusRequest)
}
