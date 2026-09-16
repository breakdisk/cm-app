package io.logisticos.driver.core.network.service

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import retrofit2.http.*

// ─── Job chat — the thread with the customer on one shipment ──────────────────
//
// Who may read or post is decided by engagement, against order-intake and
// driver-ops. A 403 here means this job is not this driver's, not that
// something broke.

@Serializable
data class JobThreadResponse(val data: JobThreadData)

@Serializable
data class JobThreadData(
    /** "driver" for this app; the field exists because the same route serves both. */
    val role: String = "driver",
    /** False once the job is over: the thread stays readable, and closed. */
    @SerialName("can_send")        val canSend: Boolean = true,
    @SerialName("shipment_status") val shipmentStatus: String? = null,
    val unread: Long = 0,
    val messages: List<JobMessageItem> = emptyList(),
)

@Serializable
data class JobMessageItem(
    val id: String,
    @SerialName("shipment_id") val shipmentId: String = "",
    @SerialName("sender_id")   val senderId: String = "",
    @SerialName("sender_role") val senderRole: String = "customer",
    val body: String = "",
    @SerialName("created_at")  val createdAt: String = "",
)

@Serializable
data class SendJobMessageRequest(
    val body: String,
    /** This phone's own id for the message, so a retry cannot post it twice. */
    @SerialName("client_message_id") val clientMessageId: String? = null,
)

@Serializable
data class JobMessageResponse(val data: JobMessageItem)

@Serializable
data class JobUnreadResponse(val data: JobUnreadData)

@Serializable
data class JobUnreadData(val unread: Long = 0)

interface EngagementApiService {

    /** GET /v1/engagement/jobs/{id}/messages — the thread, oldest first. */
    @GET("v1/engagement/jobs/{shipmentId}/messages")
    suspend fun listMessages(
        @Path("shipmentId") shipmentId: String,
        @Query("since") since: String? = null,
    ): JobThreadResponse

    /** POST /v1/engagement/jobs/{id}/messages */
    @POST("v1/engagement/jobs/{shipmentId}/messages")
    suspend fun sendMessage(
        @Path("shipmentId") shipmentId: String,
        @Body body: SendJobMessageRequest,
    ): JobMessageResponse

    /** POST /v1/engagement/jobs/{id}/messages/read — moves this driver's marker. */
    @POST("v1/engagement/jobs/{shipmentId}/messages/read")
    suspend fun markRead(@Path("shipmentId") shipmentId: String, @Body body: Map<String, String> = emptyMap())

    /** GET /v1/engagement/jobs/{id}/messages/unread — the badge; marks nothing. */
    @GET("v1/engagement/jobs/{shipmentId}/messages/unread")
    suspend fun unread(@Path("shipmentId") shipmentId: String): JobUnreadResponse
}
