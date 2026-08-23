package net.cargomarket.omnideliv.courier.data

import kotlinx.serialization.SerialName
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.Serializable
import retrofit2.Response
import retrofit2.http.Body
import retrofit2.http.GET
import retrofit2.http.POST
import okhttp3.MultipartBody
import retrofit2.http.Multipart
import retrofit2.http.Part
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

    // ── Auth ────────────────────────────────────────────────────────────────
    //
    // Phone OTP, auto-registering on first verify, because that is what the
    // platform already does — a second bespoke auth flow is what ADR-0009
    // rule 4 forbids. So there is no sign-up screen to build.

    @POST("v1/auth/otp/send")
    // Response body is `{"data":{"message":"OTP sent."}}`; only the status
    // matters, and JsonElement accepts any shape without a DTO to drift.
    suspend fun sendOtp(@Body body: OtpSendRequest): Response<kotlinx.serialization.json.JsonElement>

    @POST("v1/auth/otp/verify")
    suspend fun verifyOtp(@Body body: OtpVerifyRequest): Response<AuthEnvelope>

    /**
     * Register as a courier. Idempotent on the user, and registers **offline**
     * so signing up does not drop somebody into the next proximity search.
     */
    @POST("v1/field-ops/couriers/register")
    suspend fun registerCourier(@Body body: RegisterCourierRequest): Response<CourierDto>

    /**
     * Go on or off duty.
     *
     * The courier is resolved from the token — there is no id in the path,
     * because a courier may only start their own shift.
     *
     * This is what puts them into the proximity search. Without it a courier
     * keeps the `offline` that registration gave them and no order can ever
     * reach them, however convincingly the toggle reads.
     */
    @POST("v1/field-ops/couriers/me/status")
    suspend fun setStatus(@Body body: SetStatusRequest): Response<Unit>

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

    /**
     * The delivery photo. Multipart because the bucket is cluster-internal —
     * a presigned URL would point somewhere a courier's phone cannot reach.
     */
    @Multipart
    @POST("v1/omnideliv/courier/jobs/{orderId}/proof")
    suspend fun uploadProof(
        @Path("orderId") orderId: String,
        @Part file: MultipartBody.Part,
    ): Response<Unit>

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
data class SetStatusRequest(val available: Boolean)

/**
 * Spend the refresh token for a new session.
 *
 * Identity retires the old token on every exchange, so this is single-use —
 * see [net.cargomarket.omnideliv.courier.data.RefreshAuthenticator].
 */
@Serializable
data class RefreshRequest(@SerialName("refresh_token") val refreshToken: String)

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

/**
 * `role` is **driver**, not customer.
 *
 * The endpoint defaults to `driver`, which happens to be right here — but it is
 * sent explicitly because the customer app has to send `customer` for the same
 * endpoint, and a default that is correct for one caller by luck is the kind of
 * thing that registers couriers as customers when the default moves.
 */
@Serializable
data class OtpSendRequest(
    /**
     * `phone_number`, not `phone`.
     *
     * Identity declares the field `phone_number` with `#[serde(default)]`, so a
     * body carrying `phone` deserialises to `None` and the service answers
     * "phone_number or email is required" — a 400 the app rendered as "that
     * number does not look right". Every number failed, including correct ones,
     * and the message blamed the courier for it.
     */
    @SerialName("phone_number") val phone: String,
    @SerialName("tenant_slug") val tenantSlug: String,
    val role: String = "driver",
)

/**
 * The field is `otp_code`, not `code`.
 *
 * `tenant_slug` is required — omitting it fails with a deserialisation error
 * that reads like a server fault rather than a missing field.
 */
@Serializable
data class OtpVerifyRequest(
    /** `phone_number` — see [OtpSendRequest]. */
    @SerialName("phone_number") val phone: String,
    @SerialName("otp_code") val otpCode: String,
    @SerialName("tenant_slug") val tenantSlug: String,
    val role: String = "driver",
)

/**
 * Identity wraps its auth responses in `{"data": ...}`; field-ops and omnideliv
 * do not.
 *
 * Modelled explicitly rather than unwrapped by an interceptor, because only two
 * endpoints on this client have the envelope — a global unwrapper would then be
 * wrong for every other call. Getting this wrong cost a release: `AuthDto` read
 * the body directly, `access_token` was therefore missing, kotlinx threw, and
 * `runCatching` reported it to the courier as "we could not reach the server".
 */
@Serializable
data class AuthEnvelope(val data: AuthDto)

@Serializable
data class AuthDto(
    @SerialName("access_token") val accessToken: String,
    @SerialName("refresh_token") val refreshToken: String? = null,
    /**
     * The field is `driver_id`, and there is no `user_id` in this response.
     *
     * It doubles as the courier id: `register_courier` sets `courier.id =
     * user_id` — the ADR-0015 collapse to one identity per field worker — so
     * this is exactly what the position-ingest route expects.
     */
    @SerialName("driver_id") val driverId: String? = null,
    @SerialName("tenant_id") val tenantId: String? = null,
    @SerialName("expires_in") val expiresIn: Int? = null,
)

@Serializable
data class RegisterCourierRequest(
    @SerialName("first_name") val firstName: String,
    @SerialName("last_name") val lastName: String,
    val phone: String,
)

@Serializable
data class CourierDto(
    val id: String,
    val status: String,
)
