package io.logisticos.driver.core.network.service

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import retrofit2.http.Body
import retrofit2.http.POST

// ── Request ───────────────────────────────────────────────────────────────────

/**
 * Mirrors `RecordScanRequest` in `services/hub-ops/src/bootstrap.rs`.
 *
 * `scan_type` values match the backend's `ScanType` serde(rename_all = "snake_case"):
 * inbound_receive | pallet_assign | outbound_load |
 * container_deconsolidate | local_sort_assign | exception_flag
 *
 * `device_timestamp` is an ISO-8601 UTC string captured at the physical scan moment
 * (hardware clock, not at network-send time) — the SLA chain-of-custody basis.
 */
@Serializable
data class RecordScanRequest(
    @SerialName("hub_id")           val hubId:           String,
    @SerialName("piece_awb")        val pieceAwb:        String,
    @SerialName("master_awb")       val masterAwb:       String,
    @SerialName("shipment_id")      val shipmentId:      String,
    @SerialName("scan_type")        val scanType:        String,
    @SerialName("device_timestamp") val deviceTimestamp: String,
    @SerialName("pallet_id")        val palletId:        String? = null,
    @SerialName("container_id")     val containerId:     String? = null,
    @SerialName("exception")        val exception:       String? = null,
)

@Serializable
data class RecordScanResponse(
    val id:               String,
    @SerialName("scan_type")        val scanType:        String,
    @SerialName("device_timestamp") val deviceTimestamp: String,
    @SerialName("server_timestamp") val serverTimestamp: String,
)

// ── Service ───────────────────────────────────────────────────────────────────

interface HubOpsApiService {
    /**
     * POST /v1/hub-transfer/scans
     * Records an immutable chain-of-custody scan event at the hub.
     */
    @POST("v1/hub-transfer/scans")
    suspend fun recordScan(@Body body: RecordScanRequest): RecordScanResponse
}
