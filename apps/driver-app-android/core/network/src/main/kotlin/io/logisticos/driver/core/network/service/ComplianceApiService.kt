package io.logisticos.driver.core.network.service

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import retrofit2.http.Body
import retrofit2.http.GET
import retrofit2.http.POST
import retrofit2.http.Path

// ─── Shapes ───────────────────────────────────────────────────────────────────
// Mirrors services/compliance/src/domain/entities/*.rs.
//
// The path prefix `api/v1/compliance/me/...` is intentional — compliance is the
// only service in the platform that uses the `/api/v1` prefix instead of `/v1`.
// (Historical: it was named earlier than the gateway routing convention
// stabilised; gateway proxies `/api/v1/compliance/*` → compliance_url.)

@Serializable
data class ComplianceProfileDto(
    val id: String,
    @SerialName("tenant_id")    val tenantId: String,
    @SerialName("entity_type")  val entityType: String,   // "driver" | "customer"
    @SerialName("entity_id")    val entityId: String,
    @SerialName("overall_status") val overallStatus: String, // pending_submission | submitted | approved | rejected | suspended
    val jurisdiction: String,
)

@Serializable
data class DocumentTypeDto(
    val id: String,
    val code: String,                                       // e.g. "drivers_license", "vehicle_registration"
    val name: String,
    @SerialName("is_required") val isRequired: Boolean,
    @SerialName("requires_expiry") val requiresExpiry: Boolean,
)

@Serializable
data class DriverDocumentDto(
    val id: String,
    @SerialName("compliance_profile_id") val profileId: String,
    @SerialName("document_type_id") val documentTypeId: String,
    @SerialName("document_number") val documentNumber: String,
    @SerialName("issue_date")  val issueDate: String? = null,
    @SerialName("expiry_date") val expiryDate: String? = null,
    val status: String,                                    // submitted | approved | rejected | superseded | expired
    @SerialName("rejection_reason") val rejectionReason: String? = null,
    @SerialName("submitted_at") val submittedAt: String,
    @SerialName("reviewed_at")  val reviewedAt: String? = null,
)

@Serializable
data class MyComplianceData(
    val profile: ComplianceProfileDto,
    @SerialName("required_types") val requiredTypes: List<DocumentTypeDto>,
    val documents: List<DriverDocumentDto>,
)

@Serializable
data class MyComplianceEnvelope(val data: MyComplianceData)

@Serializable
data class UploadDocumentRequest(
    @SerialName("document_type_code") val documentTypeCode: String,
    @SerialName("document_number")    val documentNumber: String,
    @SerialName("file_base64")        val fileBase64: String,
    @SerialName("content_type")       val contentType: String,        // image/jpeg | image/png | application/pdf
    @SerialName("issue_date")         val issueDate: String? = null,  // YYYY-MM-DD
    @SerialName("expiry_date")        val expiryDate: String? = null,
)

@Serializable
data class UploadDocumentResponse(val data: DriverDocumentDto)

// ─── Retrofit interface ───────────────────────────────────────────────────────

interface ComplianceApiService {

    /** GET /api/v1/compliance/me/profile — driver's compliance profile + required docs + submitted docs. */
    @GET("api/v1/compliance/me/profile")
    suspend fun getMyProfile(): MyComplianceEnvelope

    /** GET /api/v1/compliance/me/documents/{id} — single document detail. */
    @GET("api/v1/compliance/me/documents/{id}")
    suspend fun getDocument(@Path("id") docId: String): UploadDocumentResponse

    /**
     * POST /api/v1/compliance/me/documents/upload
     *
     * Accepts base64-encoded file + metadata. Server decodes, uploads to S3,
     * creates the DriverDocument record and supersedes any prior non-final
     * submission of the same document_type. Idempotent on retry by virtue of
     * the supersede policy.
     */
    @POST("api/v1/compliance/me/documents/upload")
    suspend fun uploadDocument(@Body body: UploadDocumentRequest): UploadDocumentResponse
}
