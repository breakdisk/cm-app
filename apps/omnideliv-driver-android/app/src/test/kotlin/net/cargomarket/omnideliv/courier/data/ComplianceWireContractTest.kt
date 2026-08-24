package net.cargomarket.omnideliv.courier.data

import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import net.cargomarket.omnideliv.courier.domain.DocumentPayload
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The compliance wire contract, parsed with the **same** [CourierJson] the app
 * ships.
 *
 * A test that builds its own `Json` can pass while the app fails — that is
 * exactly how the identity auth envelope survived to production once already:
 * `ignoreUnknownKeys` silently discarded `data`, `access_token` went missing,
 * kotlinx threw, `runCatching` caught it, and the courier was told the server
 * was unreachable while it had answered 200.
 */
class ComplianceWireContractTest {

    private val json: Json = CourierJson

    /**
     * A real `GET /api/v1/compliance/me/profile` body.
     *
     * Note the `data` wrapper. Compliance envelopes **every** response, unlike
     * field-ops and omnideliv which are flat — which is why this client models
     * envelopes per-endpoint rather than unwrapping globally.
     */
    private val profileBody = """
    {
      "data": {
        "profile": {
          "id": "0b8b2a1e-0000-4000-8000-000000000001",
          "tenant_id": "00000000-0000-0000-0000-000000000001",
          "entity_type": "driver",
          "entity_id": "0b8b2a1e-0000-4000-8000-000000000009",
          "overall_status": "pending_submission",
          "jurisdiction": "PH",
          "last_reviewed_at": null,
          "reviewed_by": null,
          "suspended_at": null,
          "created_at": "2026-08-24T09:00:00Z",
          "updated_at": "2026-08-24T09:00:00Z"
        },
        "required_types": [
          {
            "id": "11111111-0000-4000-8000-000000000001",
            "code": "PH_LTO_LICENSE",
            "jurisdiction": "PH",
            "applicable_to": ["driver"],
            "name": "LTO Driving License",
            "description": null,
            "is_required": true,
            "has_expiry": true,
            "warn_days_before": 30,
            "grace_period_days": 7,
            "vehicle_classes": null
          }
        ],
        "documents": [
          {
            "id": "22222222-0000-4000-8000-000000000001",
            "compliance_profile_id": "0b8b2a1e-0000-4000-8000-000000000001",
            "document_type_id": "11111111-0000-4000-8000-000000000001",
            "document_number": "D12-34-567890",
            "issue_date": "2024-01-05",
            "expiry_date": "2029-01-05",
            "file_url": "s3://compliance/x.webp",
            "status": "approved",
            "rejection_reason": null,
            "reviewed_by": null,
            "reviewed_at": null,
            "submitted_at": "2026-08-20T10:00:00Z",
            "updated_at": "2026-08-20T10:00:00Z"
          }
        ]
      }
    }
    """.trimIndent()

    @Test
    fun a_real_profile_response_parses() {
        val env = json.decodeFromString<MyComplianceEnvelope>(profileBody)
        val d = env.data

        assertEquals("pending_submission", d.profile.overallStatus)
        assertEquals("PH", d.profile.jurisdiction)
        assertEquals(1, d.requiredTypes.size)
        assertEquals("PH_LTO_LICENSE", d.requiredTypes[0].code)
        assertTrue(d.requiredTypes[0].hasExpiry)
        assertEquals(30, d.requiredTypes[0].warnDaysBefore)
        assertEquals(1, d.documents.size)
        assertEquals("approved", d.documents[0].status)
        assertEquals("2029-01-05", d.documents[0].expiryDate)
    }

    /**
     * The failure this whole envelope discipline exists to prevent: reading the
     * body as the payload rather than as `{"data": …}`.
     */
    @Test
    fun the_payload_is_not_at_the_top_level() {
        val top = json.parseToJsonElement(profileBody).jsonObject
        assertTrue("compliance wraps in data", top.containsKey("data"))
        assertFalse("nothing useful sits at the top level", top.containsKey("profile"))
    }

    /**
     * The server sends far more fields than this client models —
     * `grace_period_days`, `file_url`, `tenant_id`. `ignoreUnknownKeys` is what
     * stops a backend deploy that adds one from breaking the app.
     */
    @Test
    fun unmodelled_server_fields_do_not_break_parsing() {
        val d = json.decodeFromString<MyComplianceEnvelope>(profileBody).data
        assertEquals("D12-34-567890", d.documents[0].documentNumber)
    }

    /** A courier with nothing submitted yet — the state every new profile is in. */
    @Test
    fun an_empty_document_list_parses() {
        val body = """
        {"data":{"profile":{"id":"a","overall_status":"pending_submission","jurisdiction":"PH"},
        "required_types":[],"documents":[]}}
        """.trimIndent()
        val d = json.decodeFromString<MyComplianceEnvelope>(body).data
        assertTrue(d.documents.isEmpty())
        assertTrue(d.requiredTypes.isEmpty())
    }

    /** A rejection carries the reviewer's note; nulls elsewhere must not throw. */
    @Test
    fun a_rejected_document_parses_with_its_reason() {
        val body = """
        {"data":{"profile":{"id":"a","overall_status":"pending_submission","jurisdiction":"PH"},
        "required_types":[],"documents":[{"id":"d","document_type_id":"t",
        "document_number":"N","status":"rejected","rejection_reason":"Photo is blurred",
        "submitted_at":"2026-08-21T00:00:00Z"}]}}
        """.trimIndent()
        val doc = json.decodeFromString<MyComplianceEnvelope>(body).data.documents[0]
        assertEquals("Photo is blurred", doc.rejectionReason)
        assertNull(doc.expiryDate)
        assertNull(doc.issueDate)
    }

    // ── Outbound ───────────────────────────────────────────────────────────

    /**
     * The keys the server's `UploadDocumentRequest` declares.
     *
     * `document_number`, `file_base64` and `content_type` have no
     * `#[serde(default)]`, so a rename here is a 422 rather than a build error —
     * the same class of failure that made every sign-in attempt fail once by
     * sending `phone` where identity declared `phone_number`.
     */
    @Test
    fun the_upload_request_uses_the_field_names_the_server_declares() {
        val body = json.encodeToString(
            UploadDocumentRequest(
                documentTypeCode = "PH_LTO_LICENSE",
                documentNumber = "D12-34-567890",
                fileBase64 = "AAAA",
                contentType = DocumentPayload.CONTENT_TYPE,
                expiryDate = "2029-01-05",
            ),
        )
        val o = json.parseToJsonElement(body).jsonObject

        listOf("document_type_code", "document_number", "file_base64", "content_type", "expiry_date")
            .forEach { assertTrue("missing $it", o.containsKey(it)) }
    }

    /**
     * A type with no expiry omits the field rather than sending an explicit
     * null. `explicitNulls = false` does that, and serde reads a missing
     * `Option` as `None` — so both halves have to stay as they are.
     */
    @Test
    fun an_absent_expiry_is_omitted_not_sent_as_null() {
        val body = json.encodeToString(
            UploadDocumentRequest(
                documentTypeCode = "PH_NBI_CLEARANCE",
                documentNumber = "N-1",
                fileBase64 = "AAAA",
                contentType = DocumentPayload.CONTENT_TYPE,
                expiryDate = null,
            ),
        )
        val o = json.parseToJsonElement(body).jsonObject
        assertFalse(o.containsKey("expiry_date"))
        assertFalse(o.containsKey("issue_date"))
    }

    /**
     * The content type is one the storage layer accepts. It rejects anything
     * outside this set outright, and the refusal arrives after the whole photo
     * has been uploaded.
     */
    @Test
    fun the_declared_content_type_is_one_the_server_accepts() {
        assertTrue(
            DocumentPayload.CONTENT_TYPE in
                setOf("image/jpeg", "image/png", "image/webp", "application/pdf"),
        )
    }

    /**
     * `document_type_code`, never `document_type_id`.
     *
     * The code is seeded by migration and identical in every environment; the
     * id is a per-database uuid. A build that sent ids would work against
     * whichever database it was tested on and fail everywhere else.
     */
    @Test
    fun the_upload_identifies_the_type_by_code_not_by_id() {
        val body = json.encodeToString(
            UploadDocumentRequest("PH_OR_CR", "N", "AAAA", DocumentPayload.CONTENT_TYPE),
        )
        val o = json.parseToJsonElement(body).jsonObject
        assertFalse(o.containsKey("document_type_id"))
        assertEquals("PH_OR_CR", o["document_type_code"].toString().trim('"'))
    }
}
