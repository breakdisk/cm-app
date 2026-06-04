package io.logisticos.driver.feature.hub.domain

/**
 * Hub chain-of-custody scan types.
 *
 * Mirrors `ScanType` in `services/hub-ops/src/domain/entities/hub_scan.rs`
 * (serde rename_all = "snake_case"). The [apiValue] is sent verbatim to the backend.
 */
enum class HubScanType(
    val apiValue: String,
    val label: String,
    val description: String,
    /** True when a pallet_id context field is required. */
    val requiresPallet: Boolean = false,
    /** True when a container_id context field is required. */
    val requiresContainer: Boolean = false,
) {
    INBOUND_RECEIVE(
        apiValue    = "inbound_receive",
        label       = "Inbound Receive",
        description = "Piece arrives at hub from a first-mile driver",
    ),
    PALLET_ASSIGN(
        apiValue        = "pallet_assign",
        label           = "Pallet Assign",
        description     = "Scan piece onto a pallet",
        requiresPallet  = true,
    ),
    OUTBOUND_LOAD(
        apiValue          = "outbound_load",
        label             = "Outbound Load",
        description       = "Load pallet or piece into a container / vehicle",
        requiresContainer = true,
    ),
    CONTAINER_DECONSOLIDATE(
        apiValue          = "container_deconsolidate",
        label             = "Break-Bulk",
        description       = "Piece broken out of a container at destination hub",
        requiresContainer = true,
    ),
    LOCAL_SORT_ASSIGN(
        apiValue    = "local_sort_assign",
        label       = "Local Sort",
        description = "Scan piece into a last-mile delivery cage or bin",
    ),
    EXCEPTION_FLAG(
        apiValue    = "exception_flag",
        label       = "Exception",
        description = "Flag parcel as missing, damaged, or weight mismatch",
    );
}
