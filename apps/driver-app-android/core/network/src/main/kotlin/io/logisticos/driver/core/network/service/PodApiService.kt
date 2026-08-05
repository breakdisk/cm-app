package io.logisticos.driver.core.network.service

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import retrofit2.http.*

// ─── Request / Response models ────────────────────────────────────────────────

@Serializable
data class InitiatePodRequest(
    @SerialName("shipment_id")      val shipmentId: String,
    @SerialName("task_id")          val taskId: String,
    @SerialName("recipient_name")   val recipientName: String,
    /** Driver's actual GPS at the moment of capture. MUST NOT be the delivery
     *  address — the server derives the geofence result from the distance
     *  between this and [deliveryLat]/[deliveryLng]. Passing the same point for
     *  both makes the 200 m gate and the 50 m OUT_OF_BOUNDS_HANDOVER flag
     *  unreachable. */
    @SerialName("capture_lat")      val captureLat: Double,
    @SerialName("capture_lng")      val captureLng: Double,
    /** Registered delivery address coordinates — the geofence anchor. */
    @SerialName("delivery_lat")     val deliveryLat: Double,
    @SerialName("delivery_lng")     val deliveryLng: Double,
    @SerialName("requires_photo")   val requiresPhoto: Boolean = true,
    @SerialName("requires_signature") val requiresSignature: Boolean = true,
    /** ISO-8601 UTC hardware-clock timestamp captured at the physical POD event.
     *  Primary time basis for SLA calculations — the server falls back to its own
     *  clock only when this is absent. */
    @SerialName("device_timestamp") val deviceTimestamp: String? = null,
)

@Serializable
data class InitiatePodResponse(
    val data: InitiatePodData
)

@Serializable
data class InitiatePodData(
    @SerialName("pod_id")            val podId: String,
    @SerialName("geofence_verified") val geofenceVerified: Boolean,
    val status: String
)

@Serializable
data class AttachSignatureRequest(
    @SerialName("signature_data") val signatureData: String   // Base64-encoded PNG
)

@Serializable
data class SubmitPodRequest(
    @SerialName("cod_collected_cents") val codCollectedCents: Long? = null,
    @SerialName("otp_code")            val otpCode: String? = null,
    /** ISO-8601 UTC hardware-clock timestamp from the physical POD event.
     *  Sent on submit as well as initiate because the pod service treats the
     *  submit-time value as authoritative when both are present. */
    @SerialName("device_timestamp")    val deviceTimestamp: String? = null,
    /**
     * Recipient's phone, denormalised from the driver's task record.
     *
     * Carried into `PodCaptured` and on into `ReconcileCodCommand`, where it is
     * what lets engagement send the `cod_receipt` WhatsApp without a
     * cross-service customer lookup. Sending it empty silently costs the
     * customer their COD receipt, and nothing errors — the consumer just has no
     * number to route to.
     */
    @SerialName("customer_phone")      val customerPhone: String = "",
)

@Serializable
data class SubmitPodResponse(
    val data: SubmitPodData
)

@Serializable
data class SubmitPodData(
    @SerialName("pod_id") val podId: String,
    val status: String
)

@Serializable
data class GenerateOtpRequest(
    @SerialName("shipment_id")      val shipmentId: String,
    @SerialName("recipient_phone")  val recipientPhone: String
)

@Serializable
data class GenerateOtpResponse(
    val data: GenerateOtpData
)

@Serializable
data class GenerateOtpData(
    @SerialName("otp_id") val otpId: String,
    /** Present when no SMS adapter is configured (dev/staging dummy mode).
     *  The app uses this to auto-verify without requiring the driver to
     *  type a code the recipient never received via SMS. */
    @SerialName("code") val code: String? = null,
)

@Serializable
data class VerifyOtpRequest(
    @SerialName("shipment_id") val shipmentId: String,
    val code: String
)

@Serializable
data class GetUploadUrlRequest(
    @SerialName("content_type") val contentType: String
)

@Serializable
data class GetUploadUrlResponse(
    val data: GetUploadUrlData
)

@Serializable
data class GetUploadUrlData(
    @SerialName("upload_url")     val uploadUrl: String,
    @SerialName("s3_key")         val s3Key: String,
    /** Headers the client MUST include in the PUT request alongside the presigned URL.
     *  Forwarded verbatim from the Rust SDK's PresignedRequest::headers() — typically
     *  contains `x-amz-content-sha256` and `x-amz-date` for Cloudflare R2. */
    @SerialName("upload_headers") val uploadHeaders: Map<String, String> = emptyMap(),
)

@Serializable
data class AttachPhotoRequest(
    @SerialName("s3_key")       val s3Key: String,
    @SerialName("content_type") val contentType: String,
    @SerialName("size_bytes")   val sizeBytes: Long
)

// ─── POP (Proof of Pickup) models ────────────────────────────────────────────

@Serializable
data class InitiatePopRequest(
    @SerialName("shipment_id")           val shipmentId: String,
    @SerialName("task_id")               val taskId: String,
    @SerialName("capture_lat")           val captureLat: Double,
    @SerialName("capture_lng")           val captureLng: Double,
    /** Pickup address coordinates — used for geofence and OUT_OF_BOUNDS_HANDOVER
     *  calculation on the server. Pass the task's lat/lng here. */
    @SerialName("pickup_lat")            val pickupLat: Double,
    @SerialName("pickup_lng")            val pickupLng: Double,
    /** ISO-8601 UTC timestamp from the driver's device clock. */
    @SerialName("device_timestamp")      val deviceTimestamp: String? = null,
    /** "balikbayan" | "standard" — drives billing track selection. */
    @SerialName("service_code")          val serviceCode: String = "standard",
    @SerialName("declared_value_cents")  val declaredValueCents: Long? = null,
)

@Serializable
data class InitiatePopResponse(
    val data: InitiatePopData
)

@Serializable
data class InitiatePopData(
    @SerialName("pop_id")                 val popId: String,
    @SerialName("geofence_verified")      val geofenceVerified: Boolean,
    @SerialName("out_of_bounds_handover") val outOfBoundsHandover: Boolean = false,
    val status: String,
)

@Serializable
data class SubmitPopRequest(
    /** Barcode value scanned from the physical AWB label.
     *  If the AWB was auto-confirmed (no real scan), pass the task AWB string. */
    @SerialName("scanned_barcode")   val scannedBarcode: String,
    @SerialName("device_timestamp")  val deviceTimestamp: String? = null,
    /** S3 key of the parcel photo uploaded via presigned URL (optional).
     *  Obtained from [PodApiService.getPopUploadUrl] and passed here so the
     *  server can attach it to the POP record in a single submit call. */
    @SerialName("photo_s3_key")      val photoS3Key: String? = null,
    @SerialName("photo_size_bytes")  val photoSizeBytes: Long? = null,
    // AR dimensioning captured in VERIFY mode (POP size / quantity / anti-fraud audit).
    // Null for manual / non-AR pickups; the pod service treats every field as optional.
    @SerialName("verified_length_cm")   val verifiedLengthCm: Double? = null,
    @SerialName("verified_width_cm")    val verifiedWidthCm: Double? = null,
    @SerialName("verified_height_cm")   val verifiedHeightCm: Double? = null,
    @SerialName("verified_cbm")         val verifiedCbm: Double? = null,
    @SerialName("volumetric_weight_kg") val volumetricWeightKg: Double? = null,
    @SerialName("box_quantity")         val boxQuantity: Int? = null,
    @SerialName("dimension_integrity")  val dimensionIntegrity: String? = null,
)

@Serializable
data class GetPodResponse(
    val data: GetPodData
)

@Serializable
data class GetPodData(
    @SerialName("pod_id")     val podId: String,
    val status: String,
    @SerialName("photo_url")  val photoUrl: String? = null,
    @SerialName("created_at") val createdAt: String? = null,
)

// ─── API interface ────────────────────────────────────────────────────────────

interface PodApiService {

    /** POST /v1/pods — initiate a POD record, returns pod_id */
    @POST("v1/pods")
    suspend fun initiate(@Body body: InitiatePodRequest): InitiatePodResponse

    /** PUT /v1/pods/{id}/signature — attach base64 signature */
    @PUT("v1/pods/{id}/signature")
    suspend fun attachSignature(
        @Path("id") podId: String,
        @Body body: AttachSignatureRequest
    )

    /**
     * POST /v1/pods/{id}/upload-url — request a pre-signed R2/S3 PUT URL.
     * The app uploads the photo bytes directly to the returned [GetUploadUrlData.uploadUrl],
     * then calls [attachPhoto] with the [GetUploadUrlData.s3Key].
     */
    @POST("v1/pods/{id}/upload-url")
    suspend fun getUploadUrl(
        @Path("id") podId: String,
        @Body body: GetUploadUrlRequest
    ): GetUploadUrlResponse

    /**
     * POST /v1/pods/{id}/photos — register a successfully uploaded photo.
     * Must be called after the direct PUT to the presigned URL succeeds.
     */
    @POST("v1/pods/{id}/photos")
    suspend fun attachPhoto(
        @Path("id") podId: String,
        @Body body: AttachPhotoRequest
    )

    /** PUT /v1/pods/{id}/submit — finalise POD; triggers TASK_COMPLETED event */
    @PUT("v1/pods/{id}/submit")
    suspend fun submit(
        @Path("id") podId: String,
        @Body body: SubmitPodRequest
    ): SubmitPodResponse

    // ── POP endpoints ──────────────────────────────────────────────────────
    // NOTE: Paths use the /v1/pod/pops prefix (not /v1/pops) because the
    // deployed API gateway only forwards requests matching /v1/pod* to the
    // pod service. The pod service exposes /pod/pops gateway-compat aliases
    // that map to the same handlers as /pops.

    /** POST /v1/pod/pops — initiate a Proof of Pickup, returns pop_id + geofence result */
    @POST("v1/pod/pops")
    suspend fun initiatePop(@Body body: InitiatePopRequest): InitiatePopResponse

    /**
     * POST /v1/pod/pops/{id}/upload-url — request a pre-signed R2 PUT URL for a pickup photo.
     * Upload photo bytes directly to the returned [GetUploadUrlData.uploadUrl], then pass
     * [GetUploadUrlData.s3Key] in [SubmitPopRequest.photoS3Key] on submit.
     */
    @POST("v1/pod/pops/{id}/upload-url")
    suspend fun getPopUploadUrl(
        @Path("id") popId: String,
        @Body body: GetUploadUrlRequest
    ): GetUploadUrlResponse

    /** PUT /v1/pod/pops/{id}/submit — finalise POP; publishes pickup.captured Kafka event */
    @PUT("v1/pod/pops/{id}/submit")
    suspend fun submitPop(
        @Path("id") popId: String,
        @Body body: SubmitPopRequest
    )

    /** GET /v1/pods/{id} — fetch POD details including photo URL */
    @GET("v1/pods/{id}")
    suspend fun get(@Path("id") podId: String): GetPodResponse

    /** POST /v1/otps/generate — send OTP SMS to recipient */
    @POST("v1/otps/generate")
    suspend fun generateOtp(@Body body: GenerateOtpRequest): GenerateOtpResponse

    /** POST /v1/otps/verify — verify the OTP entered by recipient */
    @POST("v1/otps/verify")
    suspend fun verifyOtp(@Body body: VerifyOtpRequest)
}
