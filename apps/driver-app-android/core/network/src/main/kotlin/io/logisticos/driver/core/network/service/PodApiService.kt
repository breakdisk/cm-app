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
    @SerialName("capture_lat")      val captureLat: Double,
    @SerialName("capture_lng")      val captureLng: Double,
    @SerialName("delivery_lat")     val deliveryLat: Double,
    @SerialName("delivery_lng")     val deliveryLng: Double,
    @SerialName("requires_photo")   val requiresPhoto: Boolean = true,
    @SerialName("requires_signature") val requiresSignature: Boolean = true,
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
    @SerialName("otp_code")            val otpCode: String? = null
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
    @SerialName("otp_id") val otpId: String
)

@Serializable
data class VerifyOtpRequest(
    @SerialName("shipment_id") val shipmentId: String,
    val code: String
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

    /** PUT /v1/pods/{id}/submit — finalise POD; triggers TASK_COMPLETED event */
    @PUT("v1/pods/{id}/submit")
    suspend fun submit(
        @Path("id") podId: String,
        @Body body: SubmitPodRequest
    ): SubmitPodResponse

    /** POST /v1/otps/generate — send OTP SMS to recipient */
    @POST("v1/otps/generate")
    suspend fun generateOtp(@Body body: GenerateOtpRequest): GenerateOtpResponse

    /** POST /v1/otps/verify — verify the OTP entered by recipient */
    @POST("v1/otps/verify")
    suspend fun verifyOtp(@Body body: VerifyOtpRequest)
}
