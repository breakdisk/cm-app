package io.logisticos.driver.core.network.service

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import retrofit2.http.*

// ─── Response models ─────────────────────────────────────────────────────────

@Serializable
data class TaskListResponse(
    val data: List<TaskItem>
)

@Serializable
data class TaskItem(
    @SerialName("task_id")            val taskId: String,
    @SerialName("shipment_id")        val shipmentId: String,
    val sequence: Int,
    val status: String,                // "pending" | "inprogress"
    @SerialName("task_type")          val taskType: String,    // "pickup" | "delivery"
    @SerialName("customer_name")      val customerName: String,
    @SerialName("customer_phone")     val customerPhone: String = "",
    val address: String,
    @SerialName("tracking_number")    val trackingNumber: String? = null,
    @SerialName("cod_amount_cents")   val codAmountCents: Long? = null,
    val lat: Double? = null,
    val lng: Double? = null,
    @SerialName("requires_photo")     val requiresPhoto: Boolean = false,
    @SerialName("requires_signature") val requiresSignature: Boolean = false,
    @SerialName("requires_otp")       val requiresOtp: Boolean = false,
)

@Serializable
data class CompleteTaskRequest(
    @SerialName("pod_id")               val podId: String? = null,
    @SerialName("cod_collected_cents")  val codCollectedCents: Long? = null
)

@Serializable
data class FailTaskRequest(
    val reason: String
)

@Serializable
data class UpdateLocationRequest(
    val lat: Double,
    val lng: Double,
    @SerialName("accuracy_m")  val accuracyM: Float? = null,
    @SerialName("speed_kmh")   val speedKmh: Float? = null,
    val heading: Float? = null,
    @SerialName("battery_pct") val batteryPct: Int? = null,
    @SerialName("recorded_at") val recordedAt: String
)

// ─── API interface ────────────────────────────────────────────────────────────

interface DriverOpsApiService {

    /** GET /v1/tasks — list pending + in-progress tasks for the authenticated driver */
    @GET("v1/tasks")
    suspend fun listMyTasks(): TaskListResponse

    /** PUT /v1/tasks/{id}/start — mark task as in-progress */
    @PUT("v1/tasks/{id}/start")
    suspend fun startTask(@Path("id") taskId: String)

    /** PUT /v1/tasks/{id}/complete — complete a task (delivery requires pod_id) */
    @PUT("v1/tasks/{id}/complete")
    suspend fun completeTask(
        @Path("id") taskId: String,
        @Body body: CompleteTaskRequest
    )

    /** PUT /v1/tasks/{id}/fail — mark task as failed */
    @PUT("v1/tasks/{id}/fail")
    suspend fun failTask(
        @Path("id") taskId: String,
        @Body body: FailTaskRequest
    )

    /** POST /v1/location — update driver GPS position */
    @POST("v1/location")
    suspend fun updateLocation(@Body body: UpdateLocationRequest)

    /** POST /v1/drivers/go-online */
    @POST("v1/drivers/go-online")
    suspend fun goOnline()

    /** POST /v1/drivers/go-offline */
    @POST("v1/drivers/go-offline")
    suspend fun goOffline()
}
