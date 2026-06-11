package io.logisticos.driver.feature.hub.presentation

import io.logisticos.driver.feature.hub.data.HubRepository
import io.logisticos.driver.feature.hub.domain.HubScanType
import org.junit.jupiter.api.Assertions.*
import org.junit.jupiter.api.Test

/**
 * Pure logic tests for [HubScanUiState.canSubmit] — no ViewModel / coroutine needed.
 * These mirror the contract enforced by [HubScanViewModel] before calling the repo.
 */
class HubScanViewModelTest {

    private fun minimalState(
        scanType: HubScanType = HubScanType.INBOUND_RECEIVE,
        hubId:    String = "hub-uuid",
        pieceAwb: String = "CM-CHILD-001",
        masterAwb: String = "CM-PHL-S0012345",
        shipmentId: String = "shipment-uuid",
        palletId:   String = "",
        containerId: String = "",
    ) = HubScanUiState(
        scanType    = scanType,
        hubId       = hubId,
        pieceAwb    = pieceAwb,
        masterAwb   = masterAwb,
        shipmentId  = shipmentId,
        palletId    = palletId,
        containerId = containerId,
        isSubmitting = false,
    )

    @Test fun `canSubmit true when all required fields filled for INBOUND_RECEIVE`() {
        assertTrue(minimalState().canSubmit)
    }

    @Test fun `canSubmit false when hubId blank`() {
        assertFalse(minimalState(hubId = "").canSubmit)
    }

    @Test fun `canSubmit false when pieceAwb blank`() {
        assertFalse(minimalState(pieceAwb = "").canSubmit)
    }

    @Test fun `canSubmit false when shipmentId blank`() {
        assertFalse(minimalState(shipmentId = "").canSubmit)
    }

    @Test fun `canSubmit false for PALLET_ASSIGN when palletId missing`() {
        assertFalse(minimalState(scanType = HubScanType.PALLET_ASSIGN, palletId = "").canSubmit)
    }

    @Test fun `canSubmit true for PALLET_ASSIGN when palletId provided`() {
        assertTrue(minimalState(scanType = HubScanType.PALLET_ASSIGN, palletId = "pallet-uuid").canSubmit)
    }

    @Test fun `canSubmit false for OUTBOUND_LOAD when containerId missing`() {
        assertFalse(minimalState(scanType = HubScanType.OUTBOUND_LOAD, containerId = "").canSubmit)
    }

    @Test fun `canSubmit true for OUTBOUND_LOAD when containerId provided`() {
        assertTrue(minimalState(scanType = HubScanType.OUTBOUND_LOAD, containerId = "container-uuid").canSubmit)
    }

    @Test fun `canSubmit false for CONTAINER_DECONSOLIDATE when containerId missing`() {
        assertFalse(minimalState(scanType = HubScanType.CONTAINER_DECONSOLIDATE, containerId = "").canSubmit)
    }

    @Test fun `canSubmit true for CONTAINER_DECONSOLIDATE when containerId provided`() {
        assertTrue(minimalState(scanType = HubScanType.CONTAINER_DECONSOLIDATE, containerId = "container-uuid").canSubmit)
    }

    @Test fun `canSubmit false when isSubmitting true`() {
        assertFalse(minimalState().copy(isSubmitting = true).canSubmit)
    }

    @Test fun `HubScanType apiValues match backend serde snake_case names`() {
        assertEquals("inbound_receive",         HubScanType.INBOUND_RECEIVE.apiValue)
        assertEquals("pallet_assign",           HubScanType.PALLET_ASSIGN.apiValue)
        assertEquals("outbound_load",           HubScanType.OUTBOUND_LOAD.apiValue)
        assertEquals("container_deconsolidate", HubScanType.CONTAINER_DECONSOLIDATE.apiValue)
        assertEquals("local_sort_assign",       HubScanType.LOCAL_SORT_ASSIGN.apiValue)
        assertEquals("exception_flag",          HubScanType.EXCEPTION_FLAG.apiValue)
    }

    @Test fun `isoFromMillis produces valid ISO-8601 UTC string`() {
        val ts = HubRepository.isoFromMillis(0L)
        assertTrue(ts.contains("1970-01-01"), "Expected UTC ISO string, got: $ts")
        assertTrue(ts.endsWith("+00:00") || ts.endsWith("Z"), "Expected offset, got: $ts")
    }

    @Test fun `EXCEPTION_FLAG apiValue is exception_flag`() {
        assertEquals("exception_flag", HubScanType.EXCEPTION_FLAG.apiValue)
    }

    @Test fun `canSubmit false for EXCEPTION_FLAG when exception blank`() {
        assertFalse(
            minimalState(scanType = HubScanType.EXCEPTION_FLAG)
                .copy(exception = "").canSubmit
        )
    }

    @Test fun `canSubmit true for EXCEPTION_FLAG when exception set to damaged`() {
        assertTrue(
            minimalState(scanType = HubScanType.EXCEPTION_FLAG)
                .copy(exception = "damaged").canSubmit
        )
    }

    @Test fun `AWB_PATTERN matches canonical CM AWB`() {
        val pattern = Regex("^CM-[A-Z]{3}-[A-Z]\\d{7}$")
        assertTrue(pattern.matches("CM-PHL-S0012345"))
        assertTrue(pattern.matches("CM-SGP-E9876543"))
        assertFalse(pattern.matches("CM-PHL-S001234"))    // too short
        assertFalse(pattern.matches("CM-PHL-S00123456"))  // too long
        assertFalse(pattern.matches("CM-phl-S0012345"))   // lowercase location
        assertFalse(pattern.matches("partial"))
    }

    @Test fun `isResolvingShipment defaults to false`() {
        assertFalse(HubScanUiState().isResolvingShipment)
    }

    @Test fun `shipmentResolveFailed defaults to false`() {
        assertFalse(HubScanUiState().shipmentResolveFailed)
    }

    @Test fun `canSubmit false when isResolvingShipment true`() {
        assertFalse(minimalState().copy(isResolvingShipment = true).canSubmit)
    }
}
