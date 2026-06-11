package io.logisticos.driver.feature.hub.data

import android.content.Context
import android.util.Log
import dagger.hilt.android.qualifiers.ApplicationContext
import io.logisticos.driver.core.database.dao.SyncQueueDao
import io.logisticos.driver.core.database.entity.SyncAction
import io.logisticos.driver.core.database.entity.SyncQueueEntity
import io.logisticos.driver.core.database.worker.OutboundSyncWorker
import io.logisticos.driver.core.network.service.HubOpsApiService
import io.logisticos.driver.core.network.service.RecordScanRequest
import io.logisticos.driver.feature.hub.domain.HubScanType
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import java.time.Instant
import java.time.ZoneOffset
import java.time.format.DateTimeFormatter
import javax.inject.Inject

/**
 * Hub chain-of-custody scan repository.
 *
 * Attempts an inline Retrofit call; on any network failure enqueues a
 * [SyncAction.HUB_SCAN] item so [OutboundSyncWorker] retries when connectivity
 * returns. The `device_timestamp` is captured by the caller at the physical
 * scan moment (hardware clock) and is NOT re-sampled here.
 *
 * @see OutboundSyncWorker for the HUB_SCAN dequeue/replay path.
 */
class HubRepository @Inject constructor(
    @ApplicationContext private val context: Context,
    private val hubApi:      HubOpsApiService,
    private val syncQueueDao: SyncQueueDao,
) {
    /**
     * Submit one hub scan event.
     *
     * @param hubId           UUID of the hub where the scan occurred.
     * @param pieceAwb        Child/piece AWB scanned by the camera/hardware scanner.
     * @param masterAwb       Master AWB (shipment tracking number) this piece belongs to.
     * @param shipmentId      UUID of the shipment resolved from the master AWB.
     * @param scanType        What operation this scan represents.
     * @param deviceTimestamp ISO-8601 UTC timestamp captured at the instant the camera
     *                        shutter / hardware scan fired (never at network-send time).
     * @param palletId        Required when [HubScanType.requiresPallet]; null otherwise.
     * @param containerId     Required when [HubScanType.requiresContainer]; null otherwise.
     * @param exception       Optional exception flag (missing / damaged / weight_mismatch).
     * @return `true` on inline success, `false` when queued for offline retry.
     */
    suspend fun recordScan(
        hubId:           String,
        pieceAwb:        String,
        masterAwb:       String,
        shipmentId:      String,
        scanType:        HubScanType,
        deviceTimestamp: String,
        palletId:        String? = null,
        containerId:     String? = null,
        exception:       String? = null,
    ): Boolean {
        val request = RecordScanRequest(
            hubId           = hubId,
            pieceAwb        = pieceAwb,
            masterAwb       = masterAwb,
            shipmentId      = shipmentId,
            scanType        = scanType.apiValue,
            deviceTimestamp = deviceTimestamp,
            palletId        = palletId,
            containerId     = containerId,
            exception       = exception,
        )
        return try {
            hubApi.recordScan(request)
            Log.d(TAG, "scan submitted inline: type=${scanType.apiValue} awb=$pieceAwb")
            true
        } catch (e: Exception) {
            Log.w(TAG, "scan inline failed — queuing HUB_SCAN: ${e.message}")
            val payload = Json.encodeToString(
                mapOf(
                    "hub_id"           to hubId,
                    "piece_awb"        to pieceAwb,
                    "master_awb"       to masterAwb,
                    "shipment_id"      to shipmentId,
                    "scan_type"        to scanType.apiValue,
                    "device_timestamp" to deviceTimestamp,
                    "pallet_id"        to (palletId ?: ""),
                    "container_id"     to (containerId ?: ""),
                    "exception"        to (exception ?: ""),
                )
            )
            syncQueueDao.enqueue(
                SyncQueueEntity(
                    action    = SyncAction.HUB_SCAN,
                    payloadJson = payload,
                    createdAt  = System.currentTimeMillis(),
                )
            )
            OutboundSyncWorker.kickOnce(context)
            false
        }
    }

    /**
     * Resolves a master AWB tracking number to its shipment UUID.
     *
     * Returns the shipment UUID string on success, or `null` when the backend
     * responds with HTTP 404 (AWB not found in parcel_inductions).
     * All other HTTP/network errors propagate as exceptions.
     *
     * @param awb Master AWB in canonical format, e.g. "CM-PHL-S0012345".
     */
    suspend fun lookupShipmentByAwb(awb: String): String? {
        return try {
            hubApi.getShipmentByAwb(awb).shipmentId
        } catch (e: retrofit2.HttpException) {
            if (e.code() == 404) null else throw e
        }
    }

    companion object {
        private const val TAG = "HubRepository"

        /** ISO-8601 UTC string from an epoch-millis hardware clock reading. */
        fun isoFromMillis(millis: Long): String =
            Instant.ofEpochMilli(millis)
                .atOffset(ZoneOffset.UTC)
                .format(DateTimeFormatter.ISO_OFFSET_DATE_TIME)
    }
}
