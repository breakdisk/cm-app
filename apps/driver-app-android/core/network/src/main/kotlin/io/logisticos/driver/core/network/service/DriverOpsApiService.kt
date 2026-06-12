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
    /** Merchant / sender display name — task-card header. Empty when unknown. */
    @SerialName("merchant_name")      val merchantName: String = "",
    /** "food" | "parcel" | "grocery" | "medicine" | "heavy" | "large". */
    @SerialName("delivery_category")  val deliveryCategory: String = "parcel",
    /** Declared weight in grams (0 = unknown). */
    @SerialName("weight_grams")       val weightGrams: Long = 0L,
    /** Full route ends — drawn as the card's route sketch background. */
    @SerialName("pickup_lat")         val pickupLat: Double? = null,
    @SerialName("pickup_lng")         val pickupLng: Double? = null,
    @SerialName("delivery_lat")       val deliveryLat: Double? = null,
    @SerialName("delivery_lng")       val deliveryLng: Double? = null,
    /** Per-task payout in cents. Non-null ONLY for part-time (gig) drivers —
     *  full-time drivers never receive a price from the backend. */
    @SerialName("payout_cents")       val payoutCents: Long? = null,
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
    /** "full_time" | "part_time" — gates the payout chip on task cards. */
    @SerialName("driver_type")    val driverType: String = "full_time",
    /** Gig rate per delivery in cents — payout chip on the offer card. */
    @SerialName("per_delivery_rate_cents") val perDeliveryRateCents: Int = 0,
    /** Decline counter — performance strip shows "Declines: n/20". */
    @SerialName("decline_count")  val declineCount: Int = 0,
    /** Customer rating aggregate (1–5); null until first rating lands. */
    @SerialName("rating_avg")     val ratingAvg: Float? = null,
    @SerialName("rating_count")   val ratingCount: Int = 0,
    /** Gig acceptance-rate inputs: broadcast offers with a client-verified
     *  impression vs offers this driver won. Passing is penalty-free — this
     *  ratio replaces the decline counter as THE gig metric. */
    @SerialName("offers_seen")    val offersSeen: Long = 0L,
    @SerialName("offers_claimed") val offersClaimed: Long = 0L,
)

// ─── Gig offer ("grab") models ───────────────────────────────────────────────

@Serializable
data class OpenOffersResponse(val data: List<OpenOfferItem>)

/** One live broadcast offer for this driver — GET /v1/offers/open. */
@Serializable
data class OpenOfferItem(
    val id: String,
    @SerialName("shipment_id")       val shipmentId: String,
    val wave: Int = 1,
    /** ISO-8601 — converted to epoch millis at the ViewModel. */
    @SerialName("expires_at")        val expiresAt: String,
    @SerialName("merchant_name")     val merchantName: String = "",
    @SerialName("delivery_category") val deliveryCategory: String = "parcel",
    @SerialName("weight_grams")      val weightGrams: Long = 0L,
    @SerialName("tracking_number")   val trackingNumber: String = "",
    @SerialName("customer_name")     val customerName: String = "",
    @SerialName("pickup_address")    val pickupAddress: String = "",
    @SerialName("delivery_address")  val deliveryAddress: String = "",
    @SerialName("cod_amount_cents")  val codAmountCents: Long? = null,
    @SerialName("pickup_lat")        val pickupLat: Double? = null,
    @SerialName("pickup_lng")        val pickupLng: Double? = null,
    @SerialName("delivery_lat")      val deliveryLat: Double? = null,
    @SerialName("delivery_lng")      val deliveryLng: Double? = null,
    /** THIS driver's contractual payout — never another driver's rate. */
    @SerialName("payout_cents")      val payoutCents: Long? = null,
)

@Serializable
data class ClaimOfferResponse(val data: ClaimOfferData)

@Serializable
data class ClaimOfferData(
    @SerialName("assignment_id") val assignmentId: String,
    @SerialName("shipment_id")   val shipmentId: String,
    @SerialName("payout_cents")  val payoutCents: Long? = null,
)

// ─── Earnings models ─────────────────────────────────────────────────────────

@Serializable
data class EarningsResponse(val data: EarningsData)

@Serializable
data class EarningsData(
    @SerialName("today_cents") val todayCents: Long = 0L,
    @SerialName("week_cents")  val weekCents: Long = 0L,
    val daily: List<DailyEarningItem> = emptyList(),
    val entries: List<EarningEntryItem> = emptyList(),
)

@Serializable
data class DailyEarningItem(
    /** Calendar day, YYYY-MM-DD (UTC). */
    val day: String,
    @SerialName("total_cents") val totalCents: Long = 0L,
    val deliveries: Long = 0L,
)

@Serializable
data class EarningEntryItem(
    @SerialName("task_id")           val taskId: String,
    @SerialName("tracking_number")   val trackingNumber: String? = null,
    @SerialName("merchant_name")     val merchantName: String = "",
    @SerialName("delivery_category") val deliveryCategory: String = "parcel",
    @SerialName("completed_at")      val completedAt: String? = null,
    @SerialName("payout_cents")      val payoutCents: Long? = null,
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

    // ── Gig offer ("grab") surface — gateway routes /v1/offers to dispatch ──

    /** GET /v1/offers/open — live offers for this driver (restart / missed-FCM recovery). */
    @GET("v1/offers/open")
    suspend fun listOpenOffers(): OpenOffersResponse

    /**
     * POST /v1/offers/:id/claim — the atomic grab. 200 = won; HTTP 409 = lost
     * the race (OFFER_TAKEN) or already busy (DRIVER_BUSY).
     */
    @POST("v1/offers/{id}/claim")
    suspend fun claimOffer(@Path("id") offerId: String): ClaimOfferResponse

    /** POST /v1/offers/:id/pass — penalty-free; never re-offered this task. */
    @POST("v1/offers/{id}/pass")
    suspend fun passOffer(@Path("id") offerId: String)

    /** POST /v1/offers/:id/seen — client-verified impression, fired once at
     *  first card render. The acceptance-rate denominator counts only these. */
    @POST("v1/offers/{id}/seen")
    suspend fun markOfferSeen(@Path("id") offerId: String)

    /** GET /v1/drivers/me/earnings — Today/Week totals + per-task history. */
    @GET("v1/drivers/me/earnings")
    suspend fun getMyEarnings(
        @Query("limit") limit: Int = 50,
        @Query("offset") offset: Int = 0,
    ): EarningsResponse
}
