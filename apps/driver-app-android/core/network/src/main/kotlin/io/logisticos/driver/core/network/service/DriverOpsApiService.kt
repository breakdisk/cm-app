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
    val notes: String? = null,
)

@Serializable
data class CompleteTaskRequest(
    @SerialName("pod_id")               val podId: String? = null,
    @SerialName("pop_id")               val popId: String? = null,
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

/**
 * Single breadcrumb item for bulk GPS flush.
 * Mirrors the single-location request but allows batching up to 200 samples
 * accumulated while the driver was offline.
 */
@Serializable
data class LocationBreadcrumb(
    val lat: Double,
    val lng: Double,
    @SerialName("accuracy_m")  val accuracyM: Float? = null,
    @SerialName("speed_kmh")   val speedKmh: Float? = null,
    val heading: Float? = null,
    @SerialName("recorded_at") val recordedAt: String,   // ISO-8601
)

@Serializable
data class BulkLocationRequest(
    val locations: List<LocationBreadcrumb>
)

@Serializable
data class RejectAssignmentRequest(
    val reason: String
)

/**
 * Minimal driver profile returned by GET /v1/drivers/me.
 * Wrapped in the standard { "data": ... } envelope.
 */
@Serializable
data class DriverProfileData(
    val id: String = "",
    @SerialName("hub_id") val hubId: String? = null,
)

@Serializable
data class DriverProfileResponse(val data: DriverProfileData)

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

    /**
     * POST /v1/location/bulk — flush offline GPS breadcrumbs accumulated while
     * the driver was offline. Called by OutboundSyncWorker on reconnect to
     * preserve chain-of-custody telemetry for the entire offline window.
     * Falls back gracefully when the backend hasn't deployed this endpoint yet.
     */
    @POST("v1/location/bulk")
    suspend fun bulkUpdateLocation(@Body body: BulkLocationRequest)

    /** POST /v1/drivers/go-online */
    @POST("v1/drivers/go-online")
    suspend fun goOnline()

    /** POST /v1/drivers/go-offline */
    @POST("v1/drivers/go-offline")
    suspend fun goOffline()

    /**
     * GET /v1/drivers/me — returns the authenticated driver's own profile.
     * Called after OTP login and on HomeScreen foreground to detect hub assignment.
     */
    @GET("v1/drivers/me")
    suspend fun getMyProfile(): DriverProfileResponse

    /** PUT /v1/assignments/:id/accept — driver accepts an incoming shipment assignment */
    @PUT("v1/assignments/{id}/accept")
    suspend fun acceptAssignment(@Path("id") assignmentId: String)

    /** PUT /v1/assignments/:id/reject — driver rejects with a reason */
    @PUT("v1/assignments/{id}/reject")
    suspend fun rejectAssignment(
        @Path("id") assignmentId: String,
        @Body body: RejectAssignmentRequest
    )
}
