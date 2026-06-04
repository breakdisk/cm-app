# Hub Scanner — Pending Gaps Design

**Date:** 2026-06-04  
**Status:** Approved  
**Scope:** `services/hub-ops` · `apps/driver-app-android/feature/hub` · `apps/driver-app-android/core/network`

---

## Background

The Hub Scanner feature (`feature/hub`, backend `hub-ops`) was shipped but left five wiring gaps that make it
unusable or unreliable in production. This spec describes each gap and the exact change required to close it.

---

## Gaps Being Closed

| ID | Severity | Summary |
|---|---|---|
| B-1 / V-2 | 🔴 Blocking | No AWB → shipment-ID lookup; agents must type raw UUIDs |
| T-1 | 🟠 High | `exception_flag` scan type missing from Android enum + UI |
| T-2 | 🟡 Medium | `CONTAINER_DECONSOLIDATE` silently omits container linkage |
| V-1 | 🟡 Medium | Camera re-fire duplicates failing scans |
| A-1 | 🟢 Low | `RecordScanResponse` does not expose the scan UUID |

---

## Gap B-1 / V-2 — AWB → Shipment ID Auto-Resolve

### Problem

`HubScanScreen` requires a **Shipment ID** (raw UUID). Real hub barcodes encode AWB strings
(`CM-PHL-S0012345`), never UUIDs. The field is a manual text box with no resolution path —
unusable at any real hub.

### Design

**Auto-resolve on AWB pattern match.** When `masterAwb` in the ViewModel matches the canonical
AWB format `^CM-[A-Z]{3}-[A-Z]\d{7}$` (15 chars, e.g. `CM-PHL-S0012345`), a backend lookup is
fired automatically. The Shipment ID field is populated without any extra agent tap. Both camera
scan and manual keyboard entry trigger resolution — the gate is the pattern, not the input source.

#### Backend — new endpoint

```
GET /v1/hub-transfer/shipment-by-awb?awb={tracking_number}
```

- **Handler:** `shipment_by_awb_handler` registered in `services/hub-ops/src/bootstrap.rs` alongside the existing scan routes.
- **Query:** `SELECT shipment_id, tracking_number FROM hub_ops.parcel_inductions WHERE tenant_id = $1 AND tracking_number = $2 LIMIT 1`
- **Responses:**
  - `200 OK` → `{ "shipment_id": "<uuid>", "master_awb": "<awb>" }`
  - `404 Not Found` → `{ "error": "AWB not found" }` — Android treats this as "allow manual entry"
- **Auth:** same tenant-scoped JWT middleware as all other hub-ops endpoints.
- **No cross-service DB join.** `parcel_inductions` is owned by hub-ops.

#### Android — networking

Add to `HubOpsApiService`:

```kotlin
@GET("v1/hub-transfer/shipment-by-awb")
suspend fun getShipmentByAwb(@Query("awb") awb: String): ShipmentByAwbResponse

data class ShipmentByAwbResponse(
    @SerializedName("shipment_id") val shipmentId: String,
    @SerializedName("master_awb")  val masterAwb: String,
)
```

#### Android — repository

Add to `HubRepository`:

```kotlin
/** Returns the shipment UUID for the given AWB, or null if not found (404). */
suspend fun lookupShipmentByAwb(awb: String): String?
```

Implementation calls `hubApi.getShipmentByAwb(awb)`, catches `HttpException(404)` and returns
`null`, rethrows all other exceptions so the ViewModel can show an error.

#### Android — ViewModel

New state fields on `HubScanUiState`:

```kotlin
val isResolvingShipment: Boolean = false
val shipmentResolveFailed: Boolean = false
```

`setMasterAwb(awb: String)` behaviour:
1. Updates `masterAwb` in state.
2. If `awb` matches `^CM-[A-Z]{3}-[A-Z]\\d{7}$`: cancel any in-flight resolve job, launch
   `resolveShipmentId(awb)`.
3. If `awb` is blank or partial: clear `shipmentId`, `isResolvingShipment`, `shipmentResolveFailed`.

`resolveShipmentId(awb)` (private coroutine):
1. Set `isResolvingShipment = true`, clear `shipmentResolveFailed`.
2. Call `repo.lookupShipmentByAwb(awb)`.
3. On non-null result: set `shipmentId = result`, clear `isResolvingShipment`.
4. On `null` (404): set `shipmentResolveFailed = true`, clear `isResolvingShipment`. Agent can type manually.
5. On exception: set `error = message`, clear `isResolvingShipment`.

#### Android — UI

The `HubField` for "Shipment ID" becomes context-aware:

| State | Field appearance |
|---|---|
| `isResolvingShipment = true` | Disabled, trailing `CircularProgressIndicator(14dp)`, text "Resolving…" |
| `shipmentId` populated | Enabled (editable override), trailing `✓` icon in green |
| `shipmentResolveFailed = true` | Enabled, amber border, placeholder "AWB not found — enter manually" |
| Default | Existing appearance (editable, standard border) |

---

## Gap T-1 — Exception Flag Scan Type

### Problem

The backend supports `ScanType::ExceptionFlag` with sub-values `missing / damaged / weight_mismatch`
in the `exception` field. Android has no `EXCEPTION_FLAG` entry in `HubScanType` and no UI to
select the sub-type. Agents cannot explicitly flag damaged parcels.

### Design

**`HubScanType.kt`** — add entry:

```kotlin
EXCEPTION_FLAG(
    apiValue          = "exception_flag",
    label             = "Exception",
    description       = "Flag parcel as missing, damaged, or weight mismatch",
    requiresPallet    = false,
    requiresContainer = false,
)
```

**`HubScanUiState`** — add:

```kotlin
val exceptionSubType: String = ""   // "missing" | "damaged" | "weight_mismatch" | ""
```

**`HubScanViewModel`** — add `setExceptionSubType(value: String)`.

**`HubScanScreen`** — when `state.scanType == HubScanType.EXCEPTION_FLAG`, render a chip row
below the scan-type selector with three options: `Missing` / `Damaged` / `Weight mismatch`.
The selected chip value is written to `exceptionSubType` in state and sent as the `exception`
field in `RecordScanRequest`. The existing `exception` field on the request model is already
present; only the UI selector is new.

**`canSubmit`** guard — when `scanType == EXCEPTION_FLAG`, require `exceptionSubType.isNotBlank()`.

---

## Gap T-2 — `CONTAINER_DECONSOLIDATE` Missing Container Linkage

### Problem

`CONTAINER_DECONSOLIDATE` has `requiresContainer = false`. Deconsolidation breaks parcels out
of a specific container; without the container ID, chain-of-custody linkage is lost.

### Fix

In `HubScanType.kt`, change `CONTAINER_DECONSOLIDATE`:

```kotlin
requiresContainer = true   // was: false
```

No other changes needed; `HubScanViewModel.canSubmit` already validates `containerId` when
`requiresContainer = true`, and `HubScanScreen` already renders the Container ID field when
`state.scanType.requiresContainer` is true.

---

## Gap V-1 — Camera Re-fire on Failed Submit

### Problem

After a failed submit (network error), `pieceAwb` is not cleared and `isSubmitting` drops to
`false`. The next camera frame immediately calls `onPieceScan()` → `submitScan()` again, creating
a tight retry loop that can generate duplicate scan records once connectivity returns.

### Fix

In `HubScanViewModel.submitScan()`, on the failure path, clear `pieceAwb`:

```kotlin
.onFailure { e ->
    _uiState.update { it.copy(
        isSubmitting   = false,
        error          = e.message,
        lastSubmitSuccess = false,
        pieceAwb       = "",   // ← clear so camera re-fire does not retrigger
    ) }
}
```

---

## Gap A-1 — `RecordScanResponse` Missing Scan UUID Exposure

### Problem

`RecordScanResponse` only maps 4 of ~12 returned fields. The scan UUID (`id`) is received on the
wire but not surfaced to callers, preventing future deduplication or confirmation UI.

### Fix

`RecordScanResponse` already has `val id: String`. No model change is needed — `id` is already
mapped. The gap is that `HubRepository.recordScan()` discards the response entirely (returns
`Boolean`). Change the return type to `String?` (the scan UUID on success, `null` on offline
queue), and update `HubScanViewModel` to store the last scan UUID in state for future use.

This is a low-priority polish item; if it adds scope, it can be deferred to a follow-up.

---

## Files to Change

### Backend (`services/hub-ops`)

| File | Change |
|---|---|
| `src/bootstrap.rs` | Register `GET /v1/hub-transfer/shipment-by-awb`; add `shipment_by_awb_handler` + `ShipmentByAwbResponse` struct |

### Android — Core Network

| File | Change |
|---|---|
| `core/network/.../HubOpsApiService.kt` | Add `getShipmentByAwb()` + `ShipmentByAwbResponse` |

### Android — Feature Hub

| File | Change |
|---|---|
| `feature/hub/.../domain/HubScanType.kt` | Add `EXCEPTION_FLAG` entry; set `CONTAINER_DECONSOLIDATE.requiresContainer = true` |
| `feature/hub/.../data/HubRepository.kt` | Add `lookupShipmentByAwb()` |
| `feature/hub/.../presentation/HubScanViewModel.kt` | Add resolve logic, `isResolvingShipment`, `shipmentResolveFailed`, `exceptionSubType`; fix V-1 pieceAwb clear |
| `feature/hub/.../ui/HubScanScreen.kt` | Shipment ID field state feedback; exception sub-type chip row |

---

## Out of Scope

- **Gap D-1** (AWB lookup depends on `parcel_inductions` rows) — addressed by the 404 fallback to manual entry; a deeper fix requires order-intake schema alignment and is tracked separately.
- **Gap A-2** (scan history `GET` endpoint not declared in Android) — no UI consumer exists yet; deferred.
- **`ScanEventEntity` confusion** (informational only) — no code change needed.

---

## Success Criteria

1. Scanning or typing a complete `CM-{TTT}-{S}{NNNNNNN}{C}` AWB into the Master AWB field automatically populates the Shipment ID field within ~1s on a live network.
2. On 404, the Shipment ID field becomes editable with an amber "not found" hint.
3. The **Exception** scan type chip is selectable; choosing `damaged/missing/weight_mismatch` enables submit and sends `exception_flag` as the `scan_type` to the backend.
4. `CONTAINER_DECONSOLIDATE` scans require and send a Container ID.
5. A camera scan that fails to submit does not immediately retrigger on the next camera frame.
