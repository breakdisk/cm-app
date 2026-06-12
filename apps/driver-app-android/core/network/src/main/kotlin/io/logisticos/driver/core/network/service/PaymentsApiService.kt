package io.logisticos.driver.core.network.service

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import retrofit2.http.GET

/**
 * Driver-scoped payments endpoints. Same gateway base URL as every other
 * service — the `/v1/cod/` prefix routes to the payments service.
 *
 * Read-only: the driver sees their COD cash position (what they owe the hub)
 * and per-shift ledger history. No money movement from the app.
 */
@Serializable
data class DriverLedgerResponse(val data: DriverLedgerData)

@Serializable
data class DriverLedgerData(
    /** Cents the driver currently owes the hub (open ledger balance). */
    @SerialName("open_balance_cents") val openBalanceCents: Long = 0L,
    @SerialName("open_entries")       val openEntries: List<LedgerEntryItem> = emptyList(),
    @SerialName("recent_ledgers")     val recentLedgers: List<LedgerSummaryItem> = emptyList(),
)

@Serializable
data class LedgerEntryItem(
    val id: String,
    /** "pickup_debit" | "cod_debit" | "remittance". */
    val kind: String,
    /** Positive = debit (driver received cash), negative = remittance credit. */
    @SerialName("amount_cents") val amountCents: Long,
    @SerialName("shipment_id")  val shipmentId: String? = null,
    val reference: String? = null,
    @SerialName("created_at")   val createdAt: String,
)

@Serializable
data class LedgerSummaryItem(
    val id: String,
    /** "open" | "reconciled". */
    val status: String,
    @SerialName("balance_cents") val balanceCents: Long,
    @SerialName("created_at")    val createdAt: String,
    @SerialName("entry_count")   val entryCount: Int = 0,
    val entries: List<LedgerEntryItem> = emptyList(),
)

interface PaymentsApiService {

    /** GET /v1/cod/driver-ledger/me — own cash position + ledger history. */
    @GET("v1/cod/driver-ledger/me")
    suspend fun getMyLedger(): DriverLedgerResponse
}
