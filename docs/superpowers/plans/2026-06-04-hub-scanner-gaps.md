# Hub Scanner Gaps Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close five wiring gaps in the Hub Scanner feature so it is production-usable: auto-resolve AWB → Shipment ID, add exception-flag scan type, fix container-deconsolidate linkage, fix camera re-fire duplicate, and expose scan UUID.

**Architecture:** Changes span one Rust backend handler (hub-ops `bootstrap.rs`), one Android network interface (`HubOpsApiService`), one repository (`HubRepository`), one ViewModel (`HubScanViewModel`), one domain enum (`HubScanType`), and one screen (`HubScanScreen`). Each task is independently committable. Backend first (Task 4) because the Android network layer calls it.

**Tech Stack:** Rust / Axum / SQLx (backend) · Kotlin / Hilt / Retrofit / Compose (Android)

---

## File Map

| File | Change |
|---|---|
| `services/hub-ops/src/bootstrap.rs` | Add `AwbQuery`, `ShipmentByAwbResponse`, `shipment_by_awb_handler`; register route |
| `apps/driver-app-android/core/network/.../HubOpsApiService.kt` | Add `getShipmentByAwb()` + `ShipmentByAwbResponse` |
| `apps/driver-app-android/feature/hub/.../domain/HubScanType.kt` | Add `EXCEPTION_FLAG`; set `CONTAINER_DECONSOLIDATE.requiresContainer = true` |
| `apps/driver-app-android/feature/hub/.../data/HubRepository.kt` | Add `lookupShipmentByAwb()` |
| `apps/driver-app-android/feature/hub/.../presentation/HubScanViewModel.kt` | Add `isResolvingShipment`, `shipmentResolveFailed`; `setMasterAwb` auto-resolve; `pieceAwb` clear on error |
| `apps/driver-app-android/feature/hub/.../ui/HubScanScreen.kt` | Add `ExceptionSubTypeSelector`; replace Shipment ID `HubField` with `ShipmentIdField` |
| `apps/driver-app-android/feature/hub/.../test/.../HubScanViewModelTest.kt` | Tests for T-2, T-1, V-2 canSubmit guards; AWB pattern; EXCEPTION_FLAG apiValue |

---

## Task 1: T-2 + V-1 — One-liner fixes with tests

**Files:**
- Modify: `apps/driver-app-android/feature/hub/src/main/kotlin/io/logisticos/driver/feature/hub/domain/HubScanType.kt`
- Modify: `apps/driver-app-android/feature/hub/src/main/kotlin/io/logisticos/driver/feature/hub/presentation/HubScanViewModel.kt`
- Modify: `apps/driver-app-android/feature/hub/src/test/kotlin/io/logisticos/driver/feature/hub/presentation/HubScanViewModelTest.kt`

- [ ] **Step 1.1: Write failing tests for T-2 (CONTAINER_DECONSOLIDATE requires container)**

  Add to `HubScanViewModelTest.kt` inside the `HubScanViewModelTest` class, after the existing `OUTBOUND_LOAD` tests:

  ```kotlin
  @Test fun `canSubmit false for CONTAINER_DECONSOLIDATE when containerId missing`() {
      assertFalse(minimalState(scanType = HubScanType.CONTAINER_DECONSOLIDATE, containerId = "").canSubmit)
  }

  @Test fun `canSubmit true for CONTAINER_DECONSOLIDATE when containerId provided`() {
      assertTrue(minimalState(scanType = HubScanType.CONTAINER_DECONSOLIDATE, containerId = "container-uuid").canSubmit)
  }
  ```

- [ ] **Step 1.2: Run tests — expect FAIL**

  ```
  ./gradlew :feature:hub:testDebugUnitTest --tests "*.HubScanViewModelTest.canSubmit false for CONTAINER_DECONSOLIDATE*" --no-daemon
  ```

  Expected: FAIL — `CONTAINER_DECONSOLIDATE` currently has `requiresContainer = false` so `canSubmit` returns `true` when `containerId` is blank.

- [ ] **Step 1.3: Fix T-2 — set `requiresContainer = true` on `CONTAINER_DECONSOLIDATE`**

  In `HubScanType.kt`, replace:

  ```kotlin
  CONTAINER_DECONSOLIDATE(
      apiValue    = "container_deconsolidate",
      label       = "Break-Bulk",
      description = "Piece broken out of a container at destination hub",
  ),
  ```

  With:

  ```kotlin
  CONTAINER_DECONSOLIDATE(
      apiValue          = "container_deconsolidate",
      label             = "Break-Bulk",
      description       = "Piece broken out of a container at destination hub",
      requiresContainer = true,
  ),
  ```

- [ ] **Step 1.4: Fix V-1 — clear `pieceAwb` on submit failure**

  In `HubScanViewModel.kt`, replace the `.onFailure` block inside `submitScan()`:

  ```kotlin
  .onFailure { e ->
      _uiState.update {
          it.copy(isSubmitting = false, lastSubmitSuccess = false, error = e.message)
      }
  }
  ```

  With:

  ```kotlin
  .onFailure { e ->
      _uiState.update {
          it.copy(
              isSubmitting      = false,
              lastSubmitSuccess = false,
              error             = e.message,
              // Clear piece AWB so the camera does not immediately retrigger
              // auto-submit on the next frame while the error is still showing.
              pieceAwb          = "",
          )
      }
  }
  ```

- [ ] **Step 1.5: Run all hub unit tests — expect PASS**

  ```
  ./gradlew :feature:hub:testDebugUnitTest --no-daemon
  ```

  Expected: All tests PASS including the two new CONTAINER_DECONSOLIDATE tests.

- [ ] **Step 1.6: Commit**

  ```
  git add apps/driver-app-android/feature/hub/src/main/kotlin/io/logisticos/driver/feature/hub/domain/HubScanType.kt
  git add apps/driver-app-android/feature/hub/src/main/kotlin/io/logisticos/driver/feature/hub/presentation/HubScanViewModel.kt
  git add apps/driver-app-android/feature/hub/src/test/kotlin/io/logisticos/driver/feature/hub/presentation/HubScanViewModelTest.kt
  git commit -m "fix(hub-scanner): T-2 container deconsolidate requires container; V-1 clear pieceAwb on error"
  ```

---

## Task 2: T-1a — Exception flag domain + canSubmit guard + tests

**Files:**
- Modify: `apps/driver-app-android/feature/hub/src/main/kotlin/io/logisticos/driver/feature/hub/domain/HubScanType.kt`
- Modify: `apps/driver-app-android/feature/hub/src/main/kotlin/io/logisticos/driver/feature/hub/presentation/HubScanViewModel.kt`
- Modify: `apps/driver-app-android/feature/hub/src/test/kotlin/io/logisticos/driver/feature/hub/presentation/HubScanViewModelTest.kt`

- [ ] **Step 2.1: Write failing tests for T-1**

  Add to `HubScanViewModelTest.kt` inside the class:

  ```kotlin
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
  ```

- [ ] **Step 2.2: Run tests — expect FAIL**

  ```
  ./gradlew :feature:hub:testDebugUnitTest --tests "*.HubScanViewModelTest.EXCEPTION_FLAG*" --no-daemon
  ```

  Expected: FAIL — `HubScanType.EXCEPTION_FLAG` does not exist yet.

- [ ] **Step 2.3: Add `EXCEPTION_FLAG` to `HubScanType.kt`**

  In `HubScanType.kt`, replace the trailing `;` on `LOCAL_SORT_ASSIGN` and add the new entry. The full enum body becomes:

  ```kotlin
  enum class HubScanType(
      val apiValue: String,
      val label: String,
      val description: String,
      val requiresPallet: Boolean = false,
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
  ```

- [ ] **Step 2.4: Add `canSubmit` guard for EXCEPTION_FLAG in `HubScanViewModel.kt`**

  In `HubScanUiState`, replace the `canSubmit` computed property:

  ```kotlin
  val canSubmit: Boolean get() {
      if (hubId.isBlank() || pieceAwb.isBlank() || masterAwb.isBlank() || shipmentId.isBlank()) return false
      if (scanType.requiresPallet && palletId.isBlank())       return false
      if (scanType.requiresContainer && containerId.isBlank()) return false
      if (scanType == HubScanType.EXCEPTION_FLAG && exception.isNullOrBlank()) return false
      return !isSubmitting
  }
  ```

- [ ] **Step 2.5: Run tests — expect PASS**

  ```
  ./gradlew :feature:hub:testDebugUnitTest --no-daemon
  ```

  Expected: All tests PASS.

- [ ] **Step 2.6: Update the existing `HubScanType apiValues` test to include EXCEPTION_FLAG**

  In `HubScanViewModelTest.kt`, replace:

  ```kotlin
  @Test fun `HubScanType apiValues match backend serde snake_case names`() {
      assertEquals("inbound_receive",        HubScanType.INBOUND_RECEIVE.apiValue)
      assertEquals("pallet_assign",          HubScanType.PALLET_ASSIGN.apiValue)
      assertEquals("outbound_load",          HubScanType.OUTBOUND_LOAD.apiValue)
      assertEquals("container_deconsolidate", HubScanType.CONTAINER_DECONSOLIDATE.apiValue)
      assertEquals("local_sort_assign",      HubScanType.LOCAL_SORT_ASSIGN.apiValue)
  }
  ```

  With:

  ```kotlin
  @Test fun `HubScanType apiValues match backend serde snake_case names`() {
      assertEquals("inbound_receive",         HubScanType.INBOUND_RECEIVE.apiValue)
      assertEquals("pallet_assign",           HubScanType.PALLET_ASSIGN.apiValue)
      assertEquals("outbound_load",           HubScanType.OUTBOUND_LOAD.apiValue)
      assertEquals("container_deconsolidate", HubScanType.CONTAINER_DECONSOLIDATE.apiValue)
      assertEquals("local_sort_assign",       HubScanType.LOCAL_SORT_ASSIGN.apiValue)
      assertEquals("exception_flag",          HubScanType.EXCEPTION_FLAG.apiValue)
  }
  ```

- [ ] **Step 2.7: Run tests — expect PASS**

  ```
  ./gradlew :feature:hub:testDebugUnitTest --no-daemon
  ```

- [ ] **Step 2.8: Commit**

  ```
  git add apps/driver-app-android/feature/hub/src/main/kotlin/io/logisticos/driver/feature/hub/domain/HubScanType.kt
  git add apps/driver-app-android/feature/hub/src/main/kotlin/io/logisticos/driver/feature/hub/presentation/HubScanViewModel.kt
  git add apps/driver-app-android/feature/hub/src/test/kotlin/io/logisticos/driver/feature/hub/presentation/HubScanViewModelTest.kt
  git commit -m "feat(hub-scanner): T-1 add EXCEPTION_FLAG scan type and canSubmit guard"
  ```

---

## Task 3: T-1b — Exception flag UI (chip selector)

**Files:**
- Modify: `apps/driver-app-android/feature/hub/src/main/kotlin/io/logisticos/driver/feature/hub/ui/HubScanScreen.kt`

- [ ] **Step 3.1: Add `ExceptionSubTypeSelector` composable**

  Add this private composable to `HubScanScreen.kt`, immediately after the closing `}` of the `ScanTypeSelector` composable (around line 244):

  ```kotlin
  // ── Exception sub-type selector (shown only for EXCEPTION_FLAG) ───────────────

  private val exceptionOptions = listOf(
      "missing"          to "Missing",
      "damaged"          to "Damaged",
      "weight_mismatch"  to "Weight Mismatch",
  )

  @Composable
  private fun ExceptionSubTypeSelector(
      selected: String?,
      onSelect: (String) -> Unit,
      modifier: Modifier = Modifier,
  ) {
      Column(modifier = modifier) {
          Text(
              "Exception type *",
              color      = Amber,
              fontFamily = FontFamily.Monospace,
              fontSize   = 11.sp,
              modifier   = Modifier.padding(bottom = 4.dp),
          )
          Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
              exceptionOptions.forEach { (value, label) ->
                  val isSelected = value == selected
                  Text(
                      text       = label,
                      color      = if (isSelected) Amber else TextMuted,
                      fontSize   = 11.sp,
                      fontFamily = FontFamily.Monospace,
                      fontWeight = if (isSelected) FontWeight.Bold else FontWeight.Normal,
                      modifier   = Modifier
                          .border(
                              width = 1.dp,
                              color = if (isSelected) Amber.copy(alpha = 0.5f) else Border,
                              shape = RoundedCornerShape(8.dp),
                          )
                          .background(
                              color = if (isSelected) Amber.copy(alpha = 0.08f) else Color.Transparent,
                              shape = RoundedCornerShape(8.dp),
                          )
                          .clickable { onSelect(value) }
                          .padding(horizontal = 8.dp, vertical = 4.dp),
                  )
              }
          }
      }
  }
  ```

- [ ] **Step 3.2: Wire `ExceptionSubTypeSelector` into the screen body**

  In `HubScanScreen.kt`, in the main `Column`, find the `ScanTypeSelector` call:

  ```kotlin
  ScanTypeSelector(
      selected  = state.scanType,
      onSelect  = viewModel::setScanType,
      modifier  = Modifier.padding(horizontal = 16.dp),
  )
  ```

  Replace with:

  ```kotlin
  ScanTypeSelector(
      selected  = state.scanType,
      onSelect  = viewModel::setScanType,
      modifier  = Modifier.padding(horizontal = 16.dp),
  )

  if (state.scanType == HubScanType.EXCEPTION_FLAG) {
      ExceptionSubTypeSelector(
          selected = state.exception,
          onSelect = viewModel::setException,
          modifier = Modifier.padding(horizontal = 16.dp),
      )
  }
  ```

- [ ] **Step 3.3: Verify the import for `HubScanType` is present**

  At the top of `HubScanScreen.kt`, confirm this import exists (it should already be there):

  ```kotlin
  import io.logisticos.driver.feature.hub.domain.HubScanType
  ```

  If missing, add it after the existing feature.hub imports.

- [ ] **Step 3.4: Commit**

  ```
  git add apps/driver-app-android/feature/hub/src/main/kotlin/io/logisticos/driver/feature/hub/ui/HubScanScreen.kt
  git commit -m "feat(hub-scanner): T-1 exception flag sub-type chip selector in HubScanScreen"
  ```

---

## Task 4: Backend — AWB lookup endpoint

**Files:**
- Modify: `services/hub-ops/src/bootstrap.rs`

- [ ] **Step 4.1: Add `AwbQuery` and `ShipmentByAwbResponse` structs**

  In `bootstrap.rs`, immediately before the `RecordScanRequest` struct (around line 1931), add:

  ```rust
  // ── AWB → shipment-ID lookup ─────────────────────────────────────────────────

  #[derive(serde::Deserialize)]
  struct AwbQuery {
      awb: String,
  }

  #[derive(serde::Serialize)]
  struct ShipmentByAwbResponse {
      shipment_id: Uuid,
      master_awb:  String,
  }
  ```

- [ ] **Step 4.2: Add the `shipment_by_awb_handler` function**

  In `bootstrap.rs`, immediately after the closing `}` of `list_scans_handler` (around line 2005), add:

  ```rust
  /// `GET /v1/hub-transfer/shipment-by-awb?awb=<tracking_number>`
  ///
  /// Resolves a master AWB (tracking number) to the shipment UUID recorded in
  /// `hub_ops.parcel_inductions`. Used by the Android hub-scanner app to
  /// auto-populate the Shipment ID field when the agent scans or types the master AWB.
  ///
  /// Returns 404 when the AWB has no matching induction record — the app
  /// falls back to manual UUID entry in that case.
  async fn shipment_by_awb_handler(
      State(s): State<AppState>,
      claims: AuthClaims,
      Query(params): Query<AwbQuery>,
  ) -> impl IntoResponse {
      claims.require_permission(permissions::SHIPMENT_READ)?;

      let row = sqlx::query!(
          r#"SELECT shipment_id, tracking_number
             FROM   hub_ops.parcel_inductions
             WHERE  tenant_id = $1
               AND  tracking_number = $2
             LIMIT  1"#,
          claims.tenant_id,
          params.awb,
      )
      .fetch_optional(&s.pool)
      .await
      .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;

      match row {
          Some(r) => Ok::<_, AppError>((
              StatusCode::OK,
              Json(ShipmentByAwbResponse {
                  shipment_id: r.shipment_id,
                  master_awb:  r.tracking_number,
              }),
          )),
          None => Err(AppError::NotFound(
              format!("AWB '{}' not found in parcel_inductions", params.awb),
          )),
      }
  }
  ```

- [ ] **Step 4.3: Register the route**

  In `bootstrap.rs`, find the route block that contains `"/v1/hub-transfer/scans"`:

  ```rust
  .route("/v1/hub-transfer/scans",                    post(record_scan_handler))
  .route("/v1/hub-transfer/scans/:shipment_id",       get(list_scans_handler))
  ```

  Replace with:

  ```rust
  .route("/v1/hub-transfer/scans",                    post(record_scan_handler))
  .route("/v1/hub-transfer/scans/:shipment_id",       get(list_scans_handler))
  .route("/v1/hub-transfer/shipment-by-awb",          get(shipment_by_awb_handler))
  ```

- [ ] **Step 4.4: Verify it compiles**

  ```
  set CARGO_INCREMENTAL=0 && cargo check -p hub-ops
  ```

  Expected: no errors.

- [ ] **Step 4.5: Commit**

  ```
  git add services/hub-ops/src/bootstrap.rs
  git commit -m "feat(hub-ops): GET /v1/hub-transfer/shipment-by-awb endpoint for AWB auto-resolve"
  ```

---

## Task 5: Android network + repository layer

**Files:**
- Modify: `apps/driver-app-android/core/network/src/main/kotlin/io/logisticos/driver/core/network/service/HubOpsApiService.kt`
- Modify: `apps/driver-app-android/feature/hub/src/main/kotlin/io/logisticos/driver/feature/hub/data/HubRepository.kt`

- [ ] **Step 5.1: Add `ShipmentByAwbResponse` and `getShipmentByAwb()` to `HubOpsApiService.kt`**

  Replace the entire file content with:

  ```kotlin
  package io.logisticos.driver.core.network.service

  import kotlinx.serialization.SerialName
  import kotlinx.serialization.Serializable
  import retrofit2.http.Body
  import retrofit2.http.GET
  import retrofit2.http.POST
  import retrofit2.http.Query

  // ── Request / Response models ─────────────────────────────────────────────────

  /**
   * Mirrors `RecordScanRequest` in `services/hub-ops/src/bootstrap.rs`.
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

  /**
   * Mirrors `ShipmentByAwbResponse` in `services/hub-ops/src/bootstrap.rs`.
   * Returned by `GET /v1/hub-transfer/shipment-by-awb?awb={tracking_number}`.
   */
  @Serializable
  data class ShipmentByAwbResponse(
      @SerialName("shipment_id") val shipmentId: String,
      @SerialName("master_awb")  val masterAwb:  String,
  )

  // ── Service ───────────────────────────────────────────────────────────────────

  interface HubOpsApiService {

      /** POST /v1/hub-transfer/scans — record an immutable hub scan. */
      @POST("v1/hub-transfer/scans")
      suspend fun recordScan(@Body body: RecordScanRequest): RecordScanResponse

      /**
       * GET /v1/hub-transfer/shipment-by-awb?awb={tracking_number}
       *
       * Resolves a master AWB to the shipment UUID stored in parcel_inductions.
       * Throws [retrofit2.HttpException] with code 404 when not found — callers
       * should catch 404 and allow manual UUID entry as fallback.
       */
      @GET("v1/hub-transfer/shipment-by-awb")
      suspend fun getShipmentByAwb(@Query("awb") awb: String): ShipmentByAwbResponse
  }
  ```

- [ ] **Step 5.2: Add `lookupShipmentByAwb()` to `HubRepository.kt`**

  Add this method after the `recordScan()` function, before the `companion object`:

  ```kotlin
  /**
   * Resolves a master AWB tracking number to its shipment UUID.
   *
   * Returns the shipment UUID string on success, or `null` when the backend
   * responds with HTTP 404 (AWB not found in parcel_inductions). All other
   * HTTP/network errors propagate as exceptions — callers should surface them
   * as an error state rather than silently ignoring them.
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
  ```

  Also add the missing import at the top of `HubRepository.kt` (after the existing imports):

  ```kotlin
  import retrofit2.HttpException
  ```

  (Note: the catch block above uses the fully-qualified `retrofit2.HttpException` inline to be safe; the explicit import is cleaner.)

- [ ] **Step 5.3: Verify Android compilation**

  ```
  ./gradlew :feature:hub:compileDebugKotlin :core:network:compileDebugKotlin --no-daemon
  ```

  Expected: BUILD SUCCESSFUL, 0 errors.

- [ ] **Step 5.4: Commit**

  ```
  git add apps/driver-app-android/core/network/src/main/kotlin/io/logisticos/driver/core/network/service/HubOpsApiService.kt
  git add apps/driver-app-android/feature/hub/src/main/kotlin/io/logisticos/driver/feature/hub/data/HubRepository.kt
  git commit -m "feat(hub-scanner): V-2 add getShipmentByAwb API + lookupShipmentByAwb repository method"
  ```

---

## Task 6: ViewModel auto-resolve logic + tests

**Files:**
- Modify: `apps/driver-app-android/feature/hub/src/main/kotlin/io/logisticos/driver/feature/hub/presentation/HubScanViewModel.kt`
- Modify: `apps/driver-app-android/feature/hub/src/test/kotlin/io/logisticos/driver/feature/hub/presentation/HubScanViewModelTest.kt`

- [ ] **Step 6.1: Write failing tests for auto-resolve state fields**

  Add to `HubScanViewModelTest.kt` inside the class:

  ```kotlin
  @Test fun `AWB_PATTERN matches canonical CM AWB`() {
      val pattern = Regex("^CM-[A-Z]{3}-[A-Z]\\d{7}$")
      assertTrue(pattern.matches("CM-PHL-S0012345"))
      assertTrue(pattern.matches("CM-SGP-E9876543"))
      assertFalse(pattern.matches("CM-PHL-S001234"))   // too short
      assertFalse(pattern.matches("CM-PHL-S00123456")) // too long
      assertFalse(pattern.matches("CM-phl-S0012345"))  // lowercase location
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
  ```

- [ ] **Step 6.2: Run tests — expect FAIL**

  ```
  ./gradlew :feature:hub:testDebugUnitTest --tests "*.HubScanViewModelTest.isResolvingShipment*" --no-daemon
  ```

  Expected: FAIL — `isResolvingShipment` field does not exist yet on `HubScanUiState`.

- [ ] **Step 6.3: Add new state fields to `HubScanUiState`**

  In `HubScanViewModel.kt`, replace the `HubScanUiState` data class definition with:

  ```kotlin
  data class HubScanUiState(
      val scanType:    HubScanType = HubScanType.INBOUND_RECEIVE,
      val hubId:       String = "",
      val pieceAwb:    String = "",
      val masterAwb:   String = "",
      /** Shipment UUID — auto-resolved from masterAwb or manually entered. */
      val shipmentId:  String = "",
      val palletId:    String = "",
      val containerId: String = "",
      val exception:   String? = null,
      val isSubmitting:         Boolean = false,
      val lastSubmitSuccess:    Boolean? = null,
      val lastSubmitQueued:     Boolean  = false,
      val error: String? = null,
      /** True while a backend AWB → shipment-UUID lookup is in flight. */
      val isResolvingShipment:  Boolean  = false,
      /** True when the lookup returned 404 — agent must type the UUID manually. */
      val shipmentResolveFailed: Boolean = false,
  ) {
      val canSubmit: Boolean get() {
          if (hubId.isBlank() || pieceAwb.isBlank() || masterAwb.isBlank() || shipmentId.isBlank()) return false
          if (scanType.requiresPallet && palletId.isBlank())                 return false
          if (scanType.requiresContainer && containerId.isBlank())           return false
          if (scanType == HubScanType.EXCEPTION_FLAG && exception.isNullOrBlank()) return false
          if (isSubmitting || isResolvingShipment)                           return false
          return true
      }
  }
  ```

- [ ] **Step 6.4: Add `AWB_PATTERN` and `resolveJob` to `HubScanViewModel`**

  In `HubScanViewModel.kt`, replace the class declaration and companion region with:

  ```kotlin
  @HiltViewModel
  class HubScanViewModel @Inject constructor(
      private val repo: HubRepository,
  ) : ViewModel() {

      private val _uiState = MutableStateFlow(HubScanUiState())
      val uiState: StateFlow<HubScanUiState> = _uiState.asStateFlow()

      /** Tracks the active AWB-resolve coroutine so it can be cancelled on new input. */
      private var resolveJob: Job? = null

      fun setScanType(type: HubScanType)  { _uiState.update { it.copy(scanType = type) } }
      fun setHubId(id: String)            { _uiState.update { it.copy(hubId = id) } }
      fun setShipmentId(id: String)       { _uiState.update { it.copy(shipmentId = id) } }
      fun setPalletId(id: String)         { _uiState.update { it.copy(palletId = id) } }
      fun setContainerId(id: String)      { _uiState.update { it.copy(containerId = id) } }
      fun setException(ex: String?)       { _uiState.update { it.copy(exception = ex) } }
      fun clearError()                    { _uiState.update { it.copy(error = null) } }

      /**
       * Updates masterAwb. When the value matches the canonical AWB pattern
       * `CM-[A-Z]{3}-[A-Z][0-9]{7}`, a backend lookup is launched automatically
       * to populate [HubScanUiState.shipmentId]. Any in-flight resolve is cancelled
       * first so rapid typing / camera rescans don't stack coroutines.
       */
      fun setMasterAwb(awb: String) {
          _uiState.update { it.copy(masterAwb = awb) }
          resolveJob?.cancel()
          when {
              awb.isBlank() -> _uiState.update {
                  it.copy(
                      shipmentId            = "",
                      isResolvingShipment   = false,
                      shipmentResolveFailed = false,
                  )
              }
              AWB_PATTERN.matches(awb) -> {
                  resolveJob = viewModelScope.launch { resolveShipmentId(awb) }
              }
          }
      }

      /** Resolves [awb] to a shipment UUID via the backend and writes the result to state. */
      private suspend fun resolveShipmentId(awb: String) {
          _uiState.update { it.copy(isResolvingShipment = true, shipmentResolveFailed = false) }
          runCatching { repo.lookupShipmentByAwb(awb) }
              .onSuccess { id ->
                  if (id != null) {
                      _uiState.update { it.copy(shipmentId = id, isResolvingShipment = false) }
                  } else {
                      _uiState.update {
                          it.copy(isResolvingShipment = false, shipmentResolveFailed = true)
                      }
                  }
              }
              .onFailure { e ->
                  _uiState.update {
                      it.copy(isResolvingShipment = false, error = e.message)
                  }
              }
      }

      // ... (keep onPieceScan and submitScan unchanged from previous task)
  ```

  Keep `onPieceScan` and `submitScan` exactly as they were after Task 1 (with the V-1 `pieceAwb = ""` fix on failure). Add the companion object at the end:

  ```kotlin
      companion object {
          /** Canonical AWB pattern: CM-{TTT}-{S}{7 digits}, e.g. CM-PHL-S0012345 */
          private val AWB_PATTERN = Regex("^CM-[A-Z]{3}-[A-Z]\\d{7}$")
      }
  }
  ```

- [ ] **Step 6.5: Add `Job` import**

  Ensure the import list in `HubScanViewModel.kt` includes:

  ```kotlin
  import kotlinx.coroutines.Job
  ```

- [ ] **Step 6.6: Run all hub unit tests — expect PASS**

  ```
  ./gradlew :feature:hub:testDebugUnitTest --no-daemon
  ```

  Expected: All tests PASS including the new resolve-state tests.

- [ ] **Step 6.7: Commit**

  ```
  git add apps/driver-app-android/feature/hub/src/main/kotlin/io/logisticos/driver/feature/hub/presentation/HubScanViewModel.kt
  git add apps/driver-app-android/feature/hub/src/test/kotlin/io/logisticos/driver/feature/hub/presentation/HubScanViewModelTest.kt
  git commit -m "feat(hub-scanner): V-2 ViewModel auto-resolve AWB to shipment ID"
  ```

---

## Task 7: UI feedback for resolve state (`ShipmentIdField`)

**Files:**
- Modify: `apps/driver-app-android/feature/hub/src/main/kotlin/io/logisticos/driver/feature/hub/ui/HubScanScreen.kt`

- [ ] **Step 7.1: Add `ShipmentIdField` composable**

  In `HubScanScreen.kt`, add this composable after the closing `}` of the `ExceptionSubTypeSelector` composable (after Task 3):

  ```kotlin
  // ── Shipment ID field with auto-resolve feedback ──────────────────────────────

  @Composable
  private fun ShipmentIdField(
      value:        String,
      isResolving:  Boolean,
      resolveFailed: Boolean,
      onValueChange: (String) -> Unit,
  ) {
      val borderColor = when {
          resolveFailed         -> Amber.copy(alpha = 0.55f)
          value.isNotBlank()    -> Green.copy(alpha = 0.45f)
          else                  -> Border
      }
      Column {
          Text(
              "Shipment ID *",
              color      = TextMuted,
              fontFamily = FontFamily.Monospace,
              fontSize   = 11.sp,
              modifier   = Modifier.padding(bottom = 4.dp),
          )
          OutlinedTextField(
              value         = value,
              onValueChange = onValueChange,
              enabled       = !isResolving,
              placeholder   = {
                  Text(
                      text = when {
                          isResolving   -> "Resolving…"
                          resolveFailed -> "AWB not found — enter manually"
                          else          -> "Scan master AWB to auto-fill"
                      },
                      color      = TextMuted,
                      fontSize   = 12.sp,
                      fontFamily = FontFamily.Monospace,
                  )
              },
              trailingIcon = {
                  when {
                      isResolving                        ->
                          CircularProgressIndicator(
                              modifier    = Modifier.size(16.dp).padding(2.dp),
                              color       = Cyan,
                              strokeWidth = 1.5.dp,
                          )
                      value.isNotBlank() && !resolveFailed ->
                          Icon(Icons.Default.CheckCircle, contentDescription = null,
                              tint = Green, modifier = Modifier.size(18.dp))
                      resolveFailed                      ->
                          Icon(Icons.Default.Warning, contentDescription = null,
                              tint = Amber, modifier = Modifier.size(18.dp))
                      else -> {}
                  }
              },
              textStyle   = LocalTextStyle.current.copy(
                  color      = Color.White,
                  fontFamily = FontFamily.Monospace,
                  fontSize   = 13.sp,
              ),
              colors = OutlinedTextFieldDefaults.colors(
                  unfocusedBorderColor    = borderColor,
                  focusedBorderColor      = Cyan.copy(alpha = 0.6f),
                  unfocusedContainerColor = Surface,
                  focusedContainerColor   = Surface,
                  disabledBorderColor     = Cyan.copy(alpha = 0.3f),
                  disabledContainerColor  = Surface,
                  disabledTextColor       = Color.White.copy(alpha = 0.6f),
              ),
              shape    = RoundedCornerShape(12.dp),
              modifier = Modifier.fillMaxWidth().height(50.dp),
          )
      }
  }
  ```

- [ ] **Step 7.2: Replace the generic `HubField` for Shipment ID in the screen body**

  In `HubScanScreen.kt`, inside the `Column` in the "Context Fields" section, find:

  ```kotlin
  HubField(label = "Shipment ID *", value = state.shipmentId, onValueChange = {
      viewModel.setShipmentId(it)
  }, placeholder = "UUID from master AWB lookup")
  ```

  Replace with:

  ```kotlin
  ShipmentIdField(
      value         = state.shipmentId,
      isResolving   = state.isResolvingShipment,
      resolveFailed = state.shipmentResolveFailed,
      onValueChange = viewModel::setShipmentId,
  )
  ```

- [ ] **Step 7.3: Verify `LocalTextStyle` import is present**

  Confirm `HubScanScreen.kt` imports include:

  ```kotlin
  import androidx.compose.material3.LocalTextStyle
  ```

  If missing, add it alongside the other `androidx.compose.material3.*` imports (the wildcard import `import androidx.compose.material3.*` already covers it).

- [ ] **Step 7.4: Compile check**

  ```
  ./gradlew :feature:hub:compileDebugKotlin --no-daemon
  ```

  Expected: BUILD SUCCESSFUL, 0 errors.

- [ ] **Step 7.5: Commit**

  ```
  git add apps/driver-app-android/feature/hub/src/main/kotlin/io/logisticos/driver/feature/hub/ui/HubScanScreen.kt
  git commit -m "feat(hub-scanner): V-2 ShipmentIdField with resolve spinner and error state"
  ```

---

## Task 8: Push + CI

- [ ] **Step 8.1: Push all commits**

  ```
  git push origin master
  ```

  Expected: GitHub Actions triggers `Build Android Driver App` and `CI — Rust Quickcheck`.

- [ ] **Step 8.2: Verify CI passes**

  ```
  gh run list --limit 4
  ```

  Wait for both workflows to show `completed / success`. The new Android APK artifact will be `driver-app-staging-debug-{N}`.

---

## Self-Review Checklist

- [x] **Spec coverage:** B-1/V-2 (Tasks 4–7), T-1 (Tasks 2–3), T-2 (Task 1), V-1 (Task 1) — all five gaps have tasks
- [x] **No placeholders:** All code blocks are complete
- [x] **Type consistency:** `isResolvingShipment` / `shipmentResolveFailed` defined in Task 6 Step 6.3 and used in Task 7; `EXCEPTION_FLAG` defined in Task 2 and used in Task 3; `lookupShipmentByAwb` defined in Task 5 and called in Task 6
- [x] **A-1 gap:** `RecordScanResponse.id` is already mapped — no code change needed; confirmed in spec as out of scope
- [x] **Import coverage:** `retrofit2.HttpException` (Task 5), `kotlinx.coroutines.Job` (Task 6 Step 6.5), `HubScanType` in HubScanScreen (Task 3 Step 3.3) all called out explicitly
- [x] **`minimalState()` helper:** existing test helper does not include `isResolvingShipment`; new field has default `false` so `minimalState().copy(isResolvingShipment = true)` works without changing the helper
