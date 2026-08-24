package net.cargomarket.omnideliv.courier.data

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import retrofit2.Response
import retrofit2.http.Body
import retrofit2.http.GET
import retrofit2.http.POST

/**
 * The compliance service — a third tier this app talks to, and the only one
 * that decides whether a courier is allowed to work at all.
 *
 * **The path prefix is `api/v1/`, not `v1/`.** Every other service on this
 * client is mounted at `v1/…`; compliance is the one exception, and the API
 * gateway routes on that literal prefix (`path.starts_with("/api/v1/compliance")`).
 * A path written to match the rest of this file would 404 through the gateway.
 *
 * **Compliance envelopes every response in `{"data": …}`.** Until now only the
 * two identity auth endpoints did, which is why this client models envelopes
 * explicitly instead of unwrapping them in an interceptor — a global unwrapper
 * would be wrong for field-ops and omnideliv, which are flat. Getting this
 * wrong once already cost a release: a missing envelope made kotlinx throw,
 * `runCatching` swallowed it, and the courier was told the server was
 * unreachable while it had answered 200.
 *
 * Compliance knows this courier as a **driver**. Its `entity_kind_for` maps
 * both the `driver` and `courier` roles onto entity type `driver`, so there is
 * one profile per field worker whatever the product calls them. Nothing on this
 * client needs to send that — it comes from the token's roles.
 */
interface ComplianceApi {

    /**
     * Everything the checklist needs, in one call: the profile's overall
     * status, the document types this jurisdiction requires, and what the
     * courier has already submitted.
     *
     * **This call creates the profile if it does not exist.** `resolve_profile`
     * is lazy, so the first time a courier opens the compliance screen is
     * frequently the moment their profile is opened. That is why a 404 here is
     * unexpected rather than routine.
     */
    @GET("api/v1/compliance/me/profile")
    suspend fun myCompliance(): Response<MyComplianceEnvelope>

    /**
     * Upload and register a document in one call.
     *
     * Base64 in JSON rather than multipart, because that is the endpoint the
     * service exposes for this flow. It inflates the payload by about a third,
     * which is why the image is re-encoded to WebP first — see
     * [net.cargomarket.omnideliv.courier.domain.ProofEncoding]. The route's body
     * cap is 16 MB and the storage layer's file cap is 10 MB; a ~300 KB WebP
     * encodes to ~400 KB of base64 and clears both by two orders of magnitude.
     */
    @POST("api/v1/compliance/me/documents/upload")
    suspend fun uploadDocument(@Body body: UploadDocumentRequest): Response<DocumentEnvelope>
}

// ── requests ─────────────────────────────────────────────────────────────────

/**
 * `document_type_code` rather than `document_type_id`.
 *
 * The server accepts either, and the code is the stable one: it is seeded by
 * migration and identical in every environment, whereas the id is a per-database
 * uuid. A build that cached ids would break the moment it pointed at staging.
 *
 * `issue_date` and `expiry_date` are omitted when null — `CourierJson` sets
 * `explicitNulls = false`, and serde reads a missing `Option` as `None`.
 */
@Serializable
data class UploadDocumentRequest(
    @SerialName("document_type_code") val documentTypeCode: String,
    @SerialName("document_number") val documentNumber: String,
    @SerialName("file_base64") val fileBase64: String,
    /**
     * Must be one of `image/jpeg`, `image/png`, `image/webp`,
     * `application/pdf`. The storage layer rejects anything else outright, and
     * this app always sends `image/webp` because that is what the encoder
     * produces.
     */
    @SerialName("content_type") val contentType: String,
    @SerialName("issue_date") val issueDate: String? = null,
    @SerialName("expiry_date") val expiryDate: String? = null,
)

// ── responses ────────────────────────────────────────────────────────────────

@Serializable
data class MyComplianceEnvelope(val data: MyComplianceDto)

@Serializable
data class DocumentEnvelope(val data: ComplianceDocumentDto)

@Serializable
data class MyComplianceDto(
    val profile: ComplianceProfileDto,
    /**
     * What this jurisdiction demands. Read from the server rather than compiled
     * in: the PH set is four documents and the UAE set is a different four, and
     * a hard-coded list here would demand an LTO licence from a courier in
     * Dubai.
     */
    @SerialName("required_types") val requiredTypes: List<ComplianceTypeDto>,
    val documents: List<ComplianceDocumentDto>,
)

@Serializable
data class ComplianceProfileDto(
    val id: String,
    /**
     * `pending_submission | under_review | compliant | expiring_soon | expired |
     * suspended | rejected`.
     *
     * Kept as a raw string and mapped in the domain layer, so a status a newer
     * backend adds cannot fail deserialisation of the whole screen — it renders
     * as unknown instead.
     */
    @SerialName("overall_status") val overallStatus: String,
    val jurisdiction: String,
)

@Serializable
data class ComplianceTypeDto(
    val id: String,
    val code: String,
    val name: String,
    val description: String? = null,
    @SerialName("is_required") val isRequired: Boolean = true,
    @SerialName("has_expiry") val hasExpiry: Boolean = false,
    @SerialName("warn_days_before") val warnDaysBefore: Int = 30,
)

@Serializable
data class ComplianceDocumentDto(
    val id: String,
    @SerialName("document_type_id") val documentTypeId: String,
    @SerialName("document_number") val documentNumber: String,
    @SerialName("issue_date") val issueDate: String? = null,
    @SerialName("expiry_date") val expiryDate: String? = null,
    /** `submitted | under_review | approved | rejected | expired | superseded`. */
    val status: String,
    @SerialName("rejection_reason") val rejectionReason: String? = null,
    @SerialName("submitted_at") val submittedAt: String,
)
