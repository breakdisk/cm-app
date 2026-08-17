package net.cargomarket.omnideliv.courier.data

import kotlinx.serialization.SerialName
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.Serializable
import retrofit2.Response
import retrofit2.http.Body
import retrofit2.http.GET
import retrofit2.http.POST
import retrofit2.http.Path

/**
 * The two tiers a courier talks to, and the split between them is the design.
 *
 * **field-ops** is the platform tier. It knows a courier holds a job and what it
 * pays; it is documented as never interpreting what the job *is*. So it serves
 * the offer list and takes the milestones.
 *
 * **omnideliv** is the product tier. It knows a stop is a pharmacy and that an
 * item is refrigerated. So it serves the manifest, and only after the claim —
 * `offer_to_nearest` fans out to several couriers, and anything on an offer
 * reaches every courier merely considered for the job.
 */
interface CourierApi {

    // ── field-ops ────────────────────────────────────────────────────────────

    @GET("v1/field-ops/assignments/mine")
    suspend fun myOffers(): Response<MyOffersDto>

    @POST("v1/field-ops/assignments/{id}/claim")
    suspend fun claim(@Path("id") assignmentId: String): Response<ClaimDto>

    @POST("v1/field-ops/assignments/{id}/arrived")
    suspend fun arrived(
        @Path("id") assignmentId: String,
        @Body body: ArrivedRequest,
    ): Response<Unit>

    @POST("v1/field-ops/assignments/{id}/collected")
    suspend fun collected(
        @Path("id") assignmentId: String,
        @Body body: CollectedRequest,
    ): Response<Unit>

    @POST("v1/field-ops/assignments/{id}/delivered")
    suspend fun delivered(
        @Path("id") assignmentId: String,
        @Body body: DeliveredRequest,
    ): Response<Unit>

    @POST("v1/field-ops/couriers/{id}/position")
    suspend fun position(
        @Path("id") courierId: String,
        @Body body: PositionRequest,
    ): Response<Unit>

    @GET("v1/field-ops/couriers/me/earnings")
    suspend fun earnings(): Response<EarningsDto>

    // ── omnideliv ────────────────────────────────────────────────────────────

    @GET("v1/omnideliv/courier/jobs/{orderId}")
    suspend fun manifest(@Path("orderId") orderId: String): Response<ManifestDto>
}

// ── requests ─────────────────────────────────────────────────────────────────

/**
 * `deviceTimestamp` is the hardware clock at the physical event, serialised the
 * moment it happened and never re-read at upload time. SLA maths uses it, so a
 * payload that sat in a dead zone must not bill those minutes to the courier.
 */
@Serializable
data class ArrivedRequest(
    @SerialName("stop_ref") val stopRef: String,
    @SerialName("device_timestamp") val deviceTimestamp: String,
)

@Serializable
data class CollectedRequest(
    @SerialName("vendor_id") val vendorId: String,
    @SerialName("device_timestamp") val deviceTimestamp: String,
)

@Serializable
data class DeliveredRequest(
    @SerialName("device_timestamp") val deviceTimestamp: String,
)

@Serializable
data class PositionRequest(
    val lat: Double,
    val lng: Double,
    @SerialName("device_timestamp") val deviceTimestamp: String,
)

// ── responses ────────────────────────────────────────────────────────────────

@Serializable
data class MyOffersDto(val offers: List<OfferDto>)

/**
 * What a courier sees before claiming.
 *
 * Deliberately thin. No addresses and nothing about the customer: this is
 * disclosed to every courier in the fanout, including the four who will not get
 * the job.
 */
@Serializable
data class OfferDto(
    @SerialName("assignment_id") val assignmentId: String,
    val product: String,
    @SerialName("external_ref") val externalRef: String,
    @SerialName("trip_cents") val tripCents: Long,
    @SerialName("tip_cents") val tipCents: Long,
    /**
     * Cash this courier will be holding if they take the job. On the offer for
     * the same reason the pay is: it changes whether someone wants it.
     */
    @SerialName("cod_amount_cents") val codAmountCents: Long = 0,
    /**
     * The product's own summary, forwarded by field-ops without being read.
     * Kept as a raw [JsonElement] because this app is its only interpreter and
     * the backend may ship a newer version than this build knows —
     * `parseOfferCard` handles that without throwing.
     */
    @SerialName("offer_card") val offerCard: JsonElement? = null,
    @SerialName("offered_at") val offeredAt: String,
)

/**
 * `won = false` covers both losing the race and the offer not being this
 * courier's — the server answers identically on purpose, so a client cannot
 * probe which assignment ids are real. Do not try to distinguish them here.
 */
@Serializable
data class ClaimDto(val won: Boolean)

@Serializable
data class ManifestDto(
    @SerialName("order_id") val orderId: String,
    val status: String,
    @SerialName("cod_amount_cents") val codAmountCents: Long,
    @SerialName("trip_cents") val tripCents: Long,
    @SerialName("tip_cents") val tipCents: Long,
    val stops: List<StopDto>,
    val dropoff: DropoffDto,
)

@Serializable
data class StopDto(
    @SerialName("stop_ref") val stopRef: String,
    val seq: Int,
    @SerialName("vendor_name") val vendorName: String,
    val address: String,
    val lat: Double,
    val lng: Double,
    val vertical: String,
    @SerialName("prep_time_minutes") val prepTimeMinutes: Int,
    @SerialName("picked_up") val pickedUp: Boolean,
    val lines: List<LineDto>,
)

@Serializable
data class LineDto(
    val qty: Int,
    @SerialName("item_name") val itemName: String,
    val modifiers: List<String>,
)

@Serializable
data class DropoffDto(
    @SerialName("stop_ref") val stopRef: String,
    val lat: Double,
    val lng: Double,
    @SerialName("customer_name") val customerName: String? = null,
    @SerialName("customer_phone") val customerPhone: String? = null,
    // Always null until the customer app grows a delivery-note field. Present
    // in the contract so adding it later is not a breaking change.
    val notes: String? = null,
)

@Serializable
data class EarningsDto(
    val period: String,
    @SerialName("balance_cents") val balanceCents: Long,
    val entries: List<EarningEntryDto>,
)

@Serializable
data class EarningEntryDto(
    val kind: String,
    /** Signed as stored. Never re-derive the sign on this side. */
    @SerialName("amount_cents") val amountCents: Long,
    @SerialName("external_ref") val externalRef: String? = null,
    val at: String,
)
