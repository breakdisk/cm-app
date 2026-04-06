package io.logisticos.driver.core.network.service

import kotlinx.serialization.Serializable
import retrofit2.http.*

@Serializable
data class ShiftResponse(
    val id: String,
    val driverId: String,
    val tenantId: String,
    val totalStops: Int,
    val tasks: List<TaskResponse>
)

@Serializable
data class TaskResponse(
    val id: String,
    val awb: String,
    val recipientName: String,
    val recipientPhone: String,
    val address: String,
    val lat: Double,
    val lng: Double,
    val stopOrder: Int,
    val requiresPhoto: Boolean,
    val requiresSignature: Boolean,
    val requiresOtp: Boolean,
    val isCod: Boolean,
    val codAmount: Double,
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
