# Hub Scanner Activation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire a full end-to-end hub scanner activation flow: admin assigns a driver as a hub scanner via the Hub Staff tab, Android detects the role on next Home screen visit (no re-login required), and Hub Mode appears with hub_id pre-filled.

**Architecture:** `hub_scanner` role lives in the identity service (rbac.rs + new PATCH endpoint); `hub_id` lives on the `driver_ops.drivers` table. After OTP login Android fetches both from `GET /v1/users/me` and the new `GET /v1/drivers/me`; `HomeViewModel` also re-syncs silently on every foreground so mid-shift assignment/revocation takes effect without a re-login. The admin portal Hub Staff tab orchestrates identity-first dual writes with rollback on partial failure.

**Tech Stack:** Rust / Axum / SQLx (backend) · Kotlin / Hilt / Retrofit / Compose (Android) · Next.js 14 / TypeScript / Tailwind (admin portal)

---

## File Map

| File | Change |
|---|---|
| `libs/auth/src/rbac.rs` | Add `hub_scanner` role |
| `services/identity/src/api/http/users.rs` | Add `patch_user_roles` handler |
| `services/identity/src/api/http/mod.rs` | Register PATCH route |
| `services/driver-ops/migrations/0010_add_hub_id_to_drivers.sql` | New migration |
| `services/driver-ops/src/domain/entities/driver.rs` | Add `hub_id` field |
| `services/driver-ops/src/application/commands/mod.rs` | Add `hub_id`, `remove_hub_id` |
| `services/driver-ops/src/infrastructure/db/driver_repo.rs` | Add hub_id to DriverRow, SELECT, INSERT, UPDATE |
| `services/driver-ops/src/application/services/driver_service.rs` | Apply hub_id patch in `update()` |
| `services/driver-ops/src/api/http/drivers.rs` | Add `hub_id` to DriverDto; add `get_me_driver` handler |
| `services/driver-ops/src/api/http/mod.rs` | Register `GET /v1/drivers/me` |
| `apps/driver-app-android/core/network/.../TokenStorage.kt` | Add hub_id + isHubScanner keys |
| `apps/driver-app-android/core/network/.../EncryptedTokenStorage.kt` | Implement new keys |
| `apps/driver-app-android/core/network/.../SessionManager.kt` | Expose new hub_id + isHubScanner methods |
| `apps/driver-app-android/core/network/.../DriverOpsApiService.kt` | Add `DriverProfileResponse` + `getMyProfile()` |
| `apps/driver-app-android/feature/auth/.../AuthRepository.kt` | Initial profile hydration after verifyOtp |
| `apps/driver-app-android/feature/home/.../HomeViewModel.kt` | Two-phase load: cache read + background re-sync |
| `apps/driver-app-android/feature/home/.../HomeScreen.kt` | Gate Hub Mode on isHubScanner; pass hubId |
| `apps/driver-app-android/app/.../navigation/ShiftNavGraph.kt` | Pass hubId to navigateToHubScan |
| `apps/admin-portal/src/lib/api/hub-staff.ts` | New API utility (identity-first dual-write) |
| `apps/admin-portal/src/app/(dashboard)/hubs/[hubId]/HubStaffTab.tsx` | New Hub Staff tab component |
| `apps/admin-portal/src/app/(dashboard)/hubs/[hubId]/HubDetailClient.tsx` | Add hub-staff tab |

---

## ── BACKEND ──────────────────────────────────────────────────────────────────

## Task 1: Identity — `hub_scanner` role in rbac.rs

**Files:**
- Modify: `libs/auth/src/rbac.rs:88-166`

- [ ] **Step 1.1: Add `hub_scanner` arm before the `_ => vec![]` catch-all**

  In `libs/auth/src/rbac.rs`, replace:
  ```rust
      _ => vec![],
  ```
  With:
  ```rust
      "hub_scanner" => vec![
          permissions::SHIPMENT_READ,
          permissions::SHIPMENT_UPDATE,
      ],
      _ => vec![],
  ```

- [ ] **Step 1.2: Cargo check**

  From `D:\LogisticOS`:
  ```
  set CARGO_INCREMENTAL=0 && cargo check -p logisticos-auth
  ```
  Expected: no errors.

- [ ] **Step 1.3: Commit**

  ```
  git add libs/auth/src/rbac.rs
  git commit -m "feat(identity): add hub_scanner role with SHIPMENT_READ + SHIPMENT_UPDATE"
  ```

---

## Task 2: Identity — `PATCH /v1/users/:id/roles` endpoint

**Files:**
- Modify: `services/identity/src/api/http/users.rs`
- Modify: `services/identity/src/api/http/mod.rs`

- [ ] **Step 2.1: Add `PatchRolesRequest` struct and `patch_user_roles` handler to `users.rs`**

  Add at the end of `services/identity/src/api/http/users.rs`:

  ```rust
  #[derive(serde::Deserialize)]
  pub struct PatchRolesRequest {
      pub role:   String,
      /// "assign" or "revoke"
      pub action: String,
  }

  /// `PATCH /v1/users/:id/roles`
  ///
  /// Grants or revokes a single named role on a user within the calling tenant.
  /// Currently supports role `"hub_scanner"` (see rbac.rs for permissions).
  ///
  /// Body: `{ "role": "hub_scanner", "action": "assign" | "revoke" }`
  /// Requires: `USERS_MANAGE` permission.
  pub async fn patch_user_roles(
      AuthClaims(claims): AuthClaims,
      Path(id): Path<Uuid>,
      State(state): State<Arc<AppState>>,
      Json(body): Json<PatchRolesRequest>,
  ) -> Result<Json<serde_json::Value>, AppError> {
      require_permission!(claims, logisticos_auth::rbac::permissions::USERS_MANAGE);

      let user_id = logisticos_types::UserId::from_uuid(id);
      let repo    = state.tenant_service.user_repo_ref();

      let mut user = repo.find_by_id(&user_id)
          .await
          .map_err(AppError::Internal)?
          .ok_or(AppError::NotFound { resource: "User", id: id.to_string() })?;

      // Tenant isolation: prevent cross-tenant role mutation.
      if user.tenant_id.inner() != claims.tenant_id {
          return Err(AppError::NotFound { resource: "User", id: id.to_string() });
      }

      match body.action.as_str() {
          "assign" => user.assign_role(&body.role),
          "revoke" => user.revoke_role(&body.role),
          other    => return Err(AppError::Validation(
              format!("Unknown action '{}'; expected 'assign' or 'revoke'", other)
          )),
      }

      repo.save(&user)
          .await
          .map_err(AppError::Internal)?;

      Ok(Json(serde_json::json!({ "data": user })))
  }
  ```

- [ ] **Step 2.2: Register the route in `mod.rs`**

  In `services/identity/src/api/http/mod.rs`, in `protected_router()`, find the line:
  ```rust
  .route("/users/:id/invite-link",     post(users::generate_invite_link))
  ```
  Add the new route immediately after:
  ```rust
  .route("/users/:id/invite-link",     post(users::generate_invite_link))
  .route("/users/:id/roles",           patch(users::patch_user_roles))
  ```

- [ ] **Step 2.3: Cargo check**

  ```
  set CARGO_INCREMENTAL=0 && cargo check -p logisticos-identity
  ```
  Expected: no errors. Fix any: `repo.save()` might not exist on the trait — if so, use `state.tenant_service.update_user_roles_direct(&user_id, &body.role, &body.action)` or check if a `save` method is accessible on the repo trait.

- [ ] **Step 2.4: Commit**

  ```
  git add services/identity/src/api/http/users.rs services/identity/src/api/http/mod.rs
  git commit -m "feat(identity): PATCH /v1/users/:id/roles for hub_scanner role management"
  ```

---

## Task 3: Driver-ops — migration + `hub_id` on Driver entity

**Files:**
- Create: `services/driver-ops/migrations/0010_add_hub_id_to_drivers.sql`
- Modify: `services/driver-ops/src/domain/entities/driver.rs`
- Modify: `services/driver-ops/src/infrastructure/db/driver_repo.rs`

- [ ] **Step 3.1: Create migration file**

  Create `services/driver-ops/migrations/0010_add_hub_id_to_drivers.sql`:
  ```sql
  -- Add hub_id to drivers for hub scanner assignment.
  -- No FK constraint — avoids cross-schema coupling with hub_ops.hubs.
  -- Application code is responsible for validating that the hub exists.
  ALTER TABLE driver_ops.drivers
      ADD COLUMN IF NOT EXISTS hub_id UUID NULL;
  ```

- [ ] **Step 3.2: Add `hub_id` field to the `Driver` struct**

  In `services/driver-ops/src/domain/entities/driver.rs`, add `pub hub_id: Option<uuid::Uuid>` after the `carrier_id` field:
  ```rust
  pub carrier_id:               Option<uuid::Uuid>,
  pub hub_id:                   Option<uuid::Uuid>,   // ← add this line
  pub created_at:               chrono::DateTime<chrono::Utc>,
  ```

- [ ] **Step 3.3: Add `hub_id` to `DriverRow` in `driver_repo.rs`**

  In `services/driver-ops/src/infrastructure/db/driver_repo.rs`, add `hub_id: Option<Uuid>` to `DriverRow` after `carrier_id`:
  ```rust
  carrier_id:               Option<Uuid>,
  hub_id:                   Option<Uuid>,   // ← add this line
  created_at:               chrono::DateTime<chrono::Utc>,
  ```

- [ ] **Step 3.4: Update `SELECT_COLUMNS` constant**

  Replace:
  ```rust
  const SELECT_COLUMNS: &str = r#"id, tenant_id, user_id, first_name, last_name, phone, status,
      lat, lng, last_location_at, vehicle_id, active_route_id, is_active,
      driver_type, per_delivery_rate_cents, cod_commission_rate_bps, zone, vehicle_type,
      carrier_id, created_at, updated_at"#;
  ```
  With:
  ```rust
  const SELECT_COLUMNS: &str = r#"id, tenant_id, user_id, first_name, last_name, phone, status,
      lat, lng, last_location_at, vehicle_id, active_route_id, is_active,
      driver_type, per_delivery_rate_cents, cod_commission_rate_bps, zone, vehicle_type,
      carrier_id, hub_id, created_at, updated_at"#;
  ```

- [ ] **Step 3.5: Update the `Driver::from(DriverRow)` conversion**

  In `driver_repo.rs`, in the `impl From<DriverRow> for Driver` block, add `hub_id: r.hub_id` after `carrier_id: r.carrier_id`:
  ```rust
  carrier_id: r.carrier_id,
  hub_id:     r.hub_id,   // ← add this line
  created_at: r.created_at,
  ```

- [ ] **Step 3.6: Update the `save()` INSERT and UPDATE in `driver_repo.rs`**

  In the `save()` method, replace:
  ```rust
  r#"INSERT INTO driver_ops.drivers
         (id, tenant_id, user_id, first_name, last_name, phone, status,
          lat, lng, last_location_at, vehicle_id, active_route_id,
          is_active, driver_type, per_delivery_rate_cents, cod_commission_rate_bps,
          zone, vehicle_type, carrier_id, created_at, updated_at)
     VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21)
     ON CONFLICT (id) DO UPDATE SET
         first_name              = EXCLUDED.first_name,
         last_name               = EXCLUDED.last_name,
         phone                   = EXCLUDED.phone,
         status                  = EXCLUDED.status,
         lat                     = EXCLUDED.lat,
         lng                     = EXCLUDED.lng,
         last_location_at        = EXCLUDED.last_location_at,
         vehicle_id              = EXCLUDED.vehicle_id,
         active_route_id         = EXCLUDED.active_route_id,
         is_active               = EXCLUDED.is_active,
         driver_type             = EXCLUDED.driver_type,
         per_delivery_rate_cents = EXCLUDED.per_delivery_rate_cents,
         cod_commission_rate_bps = EXCLUDED.cod_commission_rate_bps,
         zone                    = EXCLUDED.zone,
         vehicle_type            = EXCLUDED.vehicle_type,
         carrier_id              = EXCLUDED.carrier_id,
         updated_at              = EXCLUDED.updated_at"#
  ```
  With (adds `hub_id` as `$22`):
  ```rust
  r#"INSERT INTO driver_ops.drivers
         (id, tenant_id, user_id, first_name, last_name, phone, status,
          lat, lng, last_location_at, vehicle_id, active_route_id,
          is_active, driver_type, per_delivery_rate_cents, cod_commission_rate_bps,
          zone, vehicle_type, carrier_id, hub_id, created_at, updated_at)
     VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22)
     ON CONFLICT (id) DO UPDATE SET
         first_name              = EXCLUDED.first_name,
         last_name               = EXCLUDED.last_name,
         phone                   = EXCLUDED.phone,
         status                  = EXCLUDED.status,
         lat                     = EXCLUDED.lat,
         lng                     = EXCLUDED.lng,
         last_location_at        = EXCLUDED.last_location_at,
         vehicle_id              = EXCLUDED.vehicle_id,
         active_route_id         = EXCLUDED.active_route_id,
         is_active               = EXCLUDED.is_active,
         driver_type             = EXCLUDED.driver_type,
         per_delivery_rate_cents = EXCLUDED.per_delivery_rate_cents,
         cod_commission_rate_bps = EXCLUDED.cod_commission_rate_bps,
         zone                    = EXCLUDED.zone,
         vehicle_type            = EXCLUDED.vehicle_type,
         carrier_id              = EXCLUDED.carrier_id,
         hub_id                  = EXCLUDED.hub_id,
         updated_at              = EXCLUDED.updated_at"#
  ```

  Then add `.bind(d.hub_id)` after `.bind(d.carrier_id)` and before `.bind(d.created_at)` in the bind chain:
  ```rust
  .bind(d.carrier_id)
  .bind(d.hub_id)        // ← add this line
  .bind(d.created_at)
  ```

- [ ] **Step 3.7: Fix `driver_service.rs` register() — set hub_id: None in the new Driver literal**

  In `services/driver-ops/src/application/services/driver_service.rs`, in the `register()` method, add `hub_id: None` after `carrier_id: cmd.carrier_id`:
  ```rust
  carrier_id: cmd.carrier_id,
  hub_id:     None,   // ← add this line
  created_at: now,
  ```

- [ ] **Step 3.8: Cargo check**

  ```
  set CARGO_INCREMENTAL=0 && cargo check -p logisticos-driver-ops
  ```
  Expected: no errors. All `Driver { .. }` struct literals in other files will need `hub_id: None` added too — fix any exhaustive-pattern errors that appear.

- [ ] **Step 3.9: Commit**

  ```
  git add services/driver-ops/migrations/0010_add_hub_id_to_drivers.sql
  git add services/driver-ops/src/domain/entities/driver.rs
  git add services/driver-ops/src/infrastructure/db/driver_repo.rs
  git add services/driver-ops/src/application/services/driver_service.rs
  git commit -m "feat(driver-ops): add hub_id column + domain field for hub scanner assignment"
  ```

---

## Task 4: Driver-ops — `UpdateDriverCommand` + `driver_service.update()` hub_id patch

**Files:**
- Modify: `services/driver-ops/src/application/commands/mod.rs`
- Modify: `services/driver-ops/src/application/services/driver_service.rs`

- [ ] **Step 4.1: Add `hub_id` and `remove_hub_id` to `UpdateDriverCommand`**

  In `services/driver-ops/src/application/commands/mod.rs`, add two fields after `carrier_id`:
  ```rust
  pub carrier_id:           Option<Uuid>,
  /// Set the driver's hub assignment. `None` = field not sent (no change).
  /// Provide a UUID to assign; pair with `remove_hub_id: Some(true)` to clear instead.
  pub hub_id:               Option<Uuid>,
  /// Set `true` to explicitly clear the driver's hub assignment.
  pub remove_hub_id:        Option<bool>,
  ```

- [ ] **Step 4.2: Apply hub_id patch in `driver_service.rs` `update()`**

  In `services/driver-ops/src/application/services/driver_service.rs`, after `if cmd.carrier_id.is_some() { driver.carrier_id = cmd.carrier_id; }` add:
  ```rust
  if cmd.remove_hub_id == Some(true) { driver.hub_id = None; }
  else if cmd.hub_id.is_some()       { driver.hub_id = cmd.hub_id; }
  ```

- [ ] **Step 4.3: Cargo check**

  ```
  set CARGO_INCREMENTAL=0 && cargo check -p logisticos-driver-ops
  ```
  Expected: no errors.

- [ ] **Step 4.4: Commit**

  ```
  git add services/driver-ops/src/application/commands/mod.rs
  git add services/driver-ops/src/application/services/driver_service.rs
  git commit -m "feat(driver-ops): UpdateDriverCommand hub_id/remove_hub_id + service patch"
  ```

---

## Task 5: Driver-ops — `GET /v1/drivers/me` + `hub_id` in DriverDto

**Files:**
- Modify: `services/driver-ops/src/api/http/drivers.rs`
- Modify: `services/driver-ops/src/api/http/mod.rs`

- [ ] **Step 5.1: Add `hub_id` to `DriverDto` and its `From` impl**

  In `services/driver-ops/src/api/http/drivers.rs`, add `hub_id: Option<Uuid>` to `DriverDto` after `carrier_id`:
  ```rust
  carrier_id: Option<Uuid>,
  hub_id:     Option<Uuid>,   // ← add this line
  created_at: chrono::DateTime<chrono::Utc>,
  ```

  In the `impl From<&Driver> for DriverDto` block, add `hub_id: d.hub_id` after `carrier_id: d.carrier_id`:
  ```rust
  carrier_id: d.carrier_id,
  hub_id:     d.hub_id,   // ← add this line
  created_at: d.created_at,
  ```

- [ ] **Step 5.2: Add `get_me_driver` handler**

  Add this function to `services/driver-ops/src/api/http/drivers.rs`, after `update_driver`:

  ```rust
  /// `GET /v1/drivers/me`
  ///
  /// Returns the authenticated driver's own profile, including `hub_id` when
  /// assigned as a hub scanner. Called by the Android app after OTP login and
  /// on every HomeScreen foreground to detect hub assignment changes.
  ///
  /// Note: driver_id == user_id by design in this system (see DriverService::register).
  pub async fn get_me_driver(
      AuthClaims(claims): AuthClaims,
      State(state): State<Arc<AppState>>,
  ) -> Result<Json<serde_json::Value>, AppError> {
      let driver_id = logisticos_types::DriverId::from_uuid(claims.user_id);
      let driver = state.driver_service.get(&driver_id).await?;
      // Tenant isolation — belt-and-suspenders guard.
      if driver.tenant_id.inner() != claims.tenant_id {
          return Err(AppError::NotFound { resource: "Driver", id: claims.user_id.to_string() });
      }
      Ok(Json(serde_json::json!({ "data": DriverDto::from(&driver) })))
  }
  ```

- [ ] **Step 5.3: Register the route in `mod.rs`**

  In `services/driver-ops/src/api/http/mod.rs`, add the `/drivers/me` route **before** the `/:id` pattern (the comment on line 81 already says "Static sub-paths must be declared before /:id"):
  ```rust
  .route("/drivers",              get(drivers::list_drivers).post(drivers::register_driver))
  .route("/drivers/summary",      get(drivers::get_summary))
  .route("/drivers/me",           get(drivers::get_me_driver))    // ← add before go-online/go-offline
  .route("/drivers/go-online",    post(drivers::go_online))
  .route("/drivers/go-offline",   post(drivers::go_offline))
  ```

- [ ] **Step 5.4: Cargo check**

  ```
  set CARGO_INCREMENTAL=0 && cargo check -p logisticos-driver-ops
  ```
  Expected: no errors.

- [ ] **Step 5.5: Commit**

  ```
  git add services/driver-ops/src/api/http/drivers.rs services/driver-ops/src/api/http/mod.rs
  git commit -m "feat(driver-ops): GET /v1/drivers/me + hub_id in DriverDto"
  ```

---

## ── ANDROID ─────────────────────────────────────────────────────────────────

## Task 6: Android — `hub_id` + `isHubScanner` in TokenStorage / SessionManager

**Files:**
- Modify: `apps/driver-app-android/core/network/src/main/kotlin/io/logisticos/driver/core/network/auth/TokenStorage.kt`
- Modify: `apps/driver-app-android/core/network/src/main/kotlin/io/logisticos/driver/core/network/auth/EncryptedTokenStorage.kt`
- Modify: `apps/driver-app-android/core/network/src/main/kotlin/io/logisticos/driver/core/network/auth/SessionManager.kt`

- [ ] **Step 6.1: Add four new methods to `TokenStorage` interface**

  Replace the entire `TokenStorage.kt` with:
  ```kotlin
  package io.logisticos.driver.core.network.auth

  interface TokenStorage {
      fun saveJwt(token: String)
      fun getJwt(): String?
      fun saveRefreshToken(token: String)
      fun getRefreshToken(): String?
      fun saveTenantId(tenantId: String)
      fun getTenantId(): String?
      fun saveDriverId(driverId: String)
      fun getDriverId(): String?
      fun saveTenantSlug(slug: String)
      fun getTenantSlug(): String?
      fun savePendingInvite(slug: String, phone: String, sig: String)
      fun getPendingInvite(): Triple<String, String, String>?
      fun clearPendingInvite()
      // Hub scanner profile
      fun saveHubId(hubId: String?)
      fun getHubId(): String?
      fun saveIsHubScanner(isHub: Boolean)
      fun isHubScanner(): Boolean
      fun clearAll()
  }
  ```

- [ ] **Step 6.2: Implement the four new methods in `EncryptedTokenStorage.kt`**

  Add two new key constants in the `companion object`:
  ```kotlin
  private const val KEY_HUB_ID        = "hub_id"
  private const val KEY_IS_HUB_SCANNER = "is_hub_scanner"
  ```

  Add the four method implementations before `override fun clearAll()`:
  ```kotlin
  override fun saveHubId(hubId: String?) {
      if (hubId != null) prefs.edit().putString(KEY_HUB_ID, hubId).apply()
      else               prefs.edit().remove(KEY_HUB_ID).apply()
  }
  override fun getHubId(): String? = prefs.getString(KEY_HUB_ID, null)
  override fun saveIsHubScanner(isHub: Boolean) = prefs.edit().putBoolean(KEY_IS_HUB_SCANNER, isHub).apply()
  override fun isHubScanner(): Boolean = prefs.getBoolean(KEY_IS_HUB_SCANNER, false)
  ```

- [ ] **Step 6.3: Add delegate methods to `SessionManager.kt`**

  Add four new methods after `fun clearPendingInvite()`:
  ```kotlin
  // Hub scanner profile — populated after OTP login and refreshed on foreground.
  fun getHubId(): String? = tokenStorage.getHubId()
  fun saveHubId(hubId: String?) = tokenStorage.saveHubId(hubId)
  fun isHubScanner(): Boolean = tokenStorage.isHubScanner()
  fun saveIsHubScanner(isHub: Boolean) = tokenStorage.saveIsHubScanner(isHub)
  ```

- [ ] **Step 6.4: Compile check**

  From `apps/driver-app-android`:
  ```
  ./gradlew :core:network:compileDebugKotlin --no-daemon 2>&1 | tail -10
  ```
  Expected: BUILD SUCCESSFUL.

- [ ] **Step 6.5: Commit**

  ```
  git add apps/driver-app-android/core/network/src/main/kotlin/io/logisticos/driver/core/network/auth/TokenStorage.kt
  git add apps/driver-app-android/core/network/src/main/kotlin/io/logisticos/driver/core/network/auth/EncryptedTokenStorage.kt
  git add apps/driver-app-android/core/network/src/main/kotlin/io/logisticos/driver/core/network/auth/SessionManager.kt
  git commit -m "feat(android): hub_id + isHubScanner keys in TokenStorage / SessionManager"
  ```

---

## Task 7: Android — `DriverOpsApiService.getMyProfile()`

**Files:**
- Modify: `apps/driver-app-android/core/network/src/main/kotlin/io/logisticos/driver/core/network/service/DriverOpsApiService.kt`

- [ ] **Step 7.1: Add `DriverProfileResponse` data class and `getMyProfile()` to `DriverOpsApiService`**

  At the top of `DriverOpsApiService.kt`, after the existing imports, add a new data class:
  ```kotlin
  import kotlinx.serialization.SerialName
  import kotlinx.serialization.Serializable
  ```
  (These imports may already exist — only add if missing.)

  Add immediately before `interface DriverOpsApiService {`:
  ```kotlin
  /**
   * Minimal driver profile returned by GET /v1/drivers/me.
   * Wrapped in the standard { "data": ... } envelope.
   */
  @Serializable
  data class DriverProfileData(
      val id:       String                    = "",
      @SerialName("hub_id") val hubId: String? = null,
  )

  @Serializable
  data class DriverProfileResponse(val data: DriverProfileData)
  ```

  Add to the `DriverOpsApiService` interface, after `goOffline()`:
  ```kotlin
  /**
   * GET /v1/drivers/me — returns the authenticated driver's own profile.
   * Called after OTP login and on HomeScreen foreground to detect hub assignment.
   */
  @GET("v1/drivers/me")
  suspend fun getMyProfile(): DriverProfileResponse
  ```

- [ ] **Step 7.2: Compile check**

  ```
  ./gradlew :core:network:compileDebugKotlin --no-daemon 2>&1 | tail -10
  ```
  Expected: BUILD SUCCESSFUL.

- [ ] **Step 7.3: Commit**

  ```
  git add apps/driver-app-android/core/network/src/main/kotlin/io/logisticos/driver/core/network/service/DriverOpsApiService.kt
  git commit -m "feat(android): DriverOpsApiService.getMyProfile() for hub_id fetch"
  ```

---

## Task 8: Android — `AuthRepository` initial profile hydration

**Files:**
- Modify: `apps/driver-app-android/feature/auth/src/main/kotlin/io/logisticos/driver/feature/auth/data/AuthRepository.kt`

- [ ] **Step 8.1: Inject `DriverOpsApiService` and add profile hydration to `verifyOtp()`**

  Replace the entire `AuthRepository.kt` with:
  ```kotlin
  package io.logisticos.driver.feature.auth.data

  import io.logisticos.driver.core.network.auth.SessionManager
  import io.logisticos.driver.core.network.service.DriverOpsApiService
  import io.logisticos.driver.core.network.service.IdentityApiService
  import io.logisticos.driver.core.network.service.OtpSendRequest
  import io.logisticos.driver.core.network.service.OtpVerifyRequest
  import okhttp3.OkHttpClient
  import javax.inject.Inject
  import javax.inject.Named
  import javax.inject.Singleton

  @Singleton
  class AuthRepository @Inject constructor(
      private val apiService:     IdentityApiService,
      private val driverOpsApi:   DriverOpsApiService,
      private val sessionManager: SessionManager,
      private val okHttpClient:   OkHttpClient,
      @Named("dev_bypass_enabled") private val devBypassEnabled: Boolean,
  ) {

      suspend fun sendOtp(
          phone:      String? = null,
          email:      String? = null,
          tenantSlug: String,
      ): Result<Unit> {
          return try {
              apiService.sendOtp(
                  OtpSendRequest(phone = phone, email = email, tenantSlug = tenantSlug, role = "driver")
              )
              Result.success(Unit)
          } catch (e: kotlinx.coroutines.CancellationException) {
              throw e
          } catch (e: Exception) {
              Result.failure(e)
          }
      }

      /**
       * Verifies the OTP and persists the session tokens, then performs an initial
       * profile hydration: fetches roles from identity and hub_id from driver-ops.
       * Both fetches are non-fatal — auth completes even if the profile services are
       * temporarily unreachable.
       */
      suspend fun verifyOtp(
          phone:      String? = null,
          otp:        String,
          email:      String? = null,
          tenantSlug: String,
      ): Result<Unit> {
          if (devBypassEnabled && otp == "123456") {
              sessionManager.saveTokens(jwt = "dev-token", refreshToken = "dev-refresh")
              sessionManager.saveTenantId(tenantSlug)
              sessionManager.saveTenantSlug(tenantSlug)
              sessionManager.saveDriverId("dev-driver-id")
              return Result.success(Unit)
          }
          return try {
              val response = apiService.verifyOtp(
                  OtpVerifyRequest(phone = phone, email = email, otp = otp, tenantSlug = tenantSlug, role = "driver")
              ).data
              sessionManager.saveTokens(jwt = response.jwt, refreshToken = response.refreshToken)
              sessionManager.saveTenantId(response.tenantId)
              sessionManager.saveTenantSlug(tenantSlug)
              sessionManager.saveDriverId(response.driverId)

              // ── Initial profile hydration ─────────────────────────────────
              // Both calls use the JWT just saved above. runCatching prevents
              // a profile service outage from blocking login.
              runCatching { apiService.getMe() }
                  .onSuccess { me ->
                      sessionManager.saveIsHubScanner(me.data.roles.contains("hub_scanner"))
                  }
              runCatching { driverOpsApi.getMyProfile() }
                  .onSuccess { r ->
                      sessionManager.saveHubId(r.data.hubId)
                  }

              Result.success(Unit)
          } catch (e: kotlinx.coroutines.CancellationException) {
              throw e
          } catch (e: Exception) {
              Result.failure(e)
          }
      }

      fun isLoggedIn(): Boolean = sessionManager.isLoggedIn()
      fun isOfflineModeActive(): Boolean = sessionManager.isOfflineModeActive()

      fun logout() {
          okHttpClient.dispatcher.cancelAll()
          sessionManager.clearSession()
      }
  }
  ```

- [ ] **Step 8.2: Compile check**

  ```
  ./gradlew :feature:auth:compileDebugKotlin --no-daemon 2>&1 | tail -10
  ```
  Expected: BUILD SUCCESSFUL.

- [ ] **Step 8.3: Commit**

  ```
  git add apps/driver-app-android/feature/auth/src/main/kotlin/io/logisticos/driver/feature/auth/data/AuthRepository.kt
  git commit -m "feat(android): AuthRepository initial hub_scanner profile hydration after verifyOtp"
  ```

---

## Task 9: Android — `HomeViewModel` two-phase profile load

**Files:**
- Modify: `apps/driver-app-android/feature/home/src/main/kotlin/io/logisticos/driver/feature/home/presentation/HomeViewModel.kt`

- [ ] **Step 9.1: Add `isHubScanner` and `hubId` to `HomeUiState`**

  In `HomeViewModel.kt`, add two fields to `HomeUiState` after `complianceStatus`:
  ```kotlin
  val complianceStatus: String? = null,
  /** True when the driver has the hub_scanner role. Gates Hub Mode button visibility. */
  val isHubScanner: Boolean = false,
  /** Hub UUID pre-filled into HubScanScreen. Empty string when no hub is assigned. */
  val hubId: String = "",
  ```

- [ ] **Step 9.2: Inject `IdentityApiService` into `HomeViewModel`**

  In `HomeViewModel.kt`, add `IdentityApiService` to the constructor (after `complianceApi`):
  ```kotlin
  @HiltViewModel
  class HomeViewModel @Inject constructor(
      @ApplicationContext private val context: Context,
      private val repo:          ShiftRepository,
      private val api:           DriverOpsApiService,
      private val complianceApi: ComplianceApiService,
      private val identityApi:   io.logisticos.driver.core.network.service.IdentityApiService,
      private val locationRepo:  LocationRepository,
      private val syncQueueDao:  SyncQueueDao,
      private val sessionManager: io.logisticos.driver.core.network.auth.SessionManager,
  ) : ViewModel() {
  ```
  Note: `DriverOpsApiService` is already injected as `api`; also inject `SessionManager` and `IdentityApiService` for the profile refresh.

- [ ] **Step 9.3: Add two-phase profile load to `init {}`**

  In the `init {}` block, after the existing `syncShift()` / `startPolling()` / etc. calls, add:
  ```kotlin
  // Phase 1 — instant render from cache
  _uiState.update { it.copy(
      isHubScanner = sessionManager.isHubScanner(),
      hubId        = sessionManager.getHubId() ?: "",
  ) }
  // Phase 2 — silent background re-sync (catches mid-shift role changes)
  refreshHubProfile()
  ```

- [ ] **Step 9.4: Add `refreshHubProfile()` private function**

  Add this private function to `HomeViewModel`:
  ```kotlin
  /**
   * Silently re-fetches role and hub_id from the backend.
   * Called in init{} and hooked into syncShift() so it runs every
   * foreground — mid-shift role changes take effect without re-login.
   * Both calls are fire-and-forget; failures leave cached values intact.
   */
  private fun refreshHubProfile() {
      viewModelScope.launch {
          runCatching { identityApi.getMe() }
              .onSuccess { me ->
                  val isHub = me.data.roles.contains("hub_scanner")
                  sessionManager.saveIsHubScanner(isHub)
                  _uiState.update { it.copy(isHubScanner = isHub) }
              }
          runCatching { api.getMyProfile() }
              .onSuccess { r ->
                  sessionManager.saveHubId(r.data.hubId)
                  _uiState.update { it.copy(hubId = r.data.hubId ?: "") }
              }
      }
  }
  ```

- [ ] **Step 9.5: Call `refreshHubProfile()` from `syncShift()`**

  In `HomeViewModel.syncShift()`, add `refreshHubProfile()` at the end:
  ```kotlin
  fun syncShift() {
      viewModelScope.launch {
          _uiState.update { it.copy(isLoading = true, error = null) }
          runCatching { repo.syncShift() }
              .onFailure { e -> _uiState.update { it.copy(error = e.message, isOfflineMode = true) } }
              .onSuccess { _uiState.update { it.copy(isOfflineMode = false) } }
          _uiState.update { it.copy(isLoading = false) }
          loadComplianceStatus()
          refreshHubProfile()   // ← add this line
      }
  }
  ```

- [ ] **Step 9.6: Compile check**

  ```
  ./gradlew :feature:home:compileDebugKotlin --no-daemon 2>&1 | tail -10
  ```
  Expected: BUILD SUCCESSFUL. Fix any Hilt injection errors by verifying `IdentityApiService` and `SessionManager` are provided in `NetworkModule` / `AuthModule`.

- [ ] **Step 9.7: Commit**

  ```
  git add apps/driver-app-android/feature/home/src/main/kotlin/io/logisticos/driver/feature/home/presentation/HomeViewModel.kt
  git commit -m "feat(android): HomeViewModel two-phase hub profile load + foreground re-sync"
  ```

---

## Task 10: Android — `HomeScreen` gating + `ShiftNavGraph` hub_id pass-through

**Files:**
- Modify: `apps/driver-app-android/feature/home/src/main/kotlin/io/logisticos/driver/feature/home/ui/HomeScreen.kt`
- Modify: `apps/driver-app-android/app/src/main/kotlin/io/logisticos/driver/navigation/ShiftNavGraph.kt`

- [ ] **Step 10.1: Change `onNavigateToHubScan` param type in `HomeScreen`**

  In `HomeScreen.kt`, change the function signature from:
  ```kotlin
  onNavigateToHubScan: () -> Unit = {},
  ```
  To:
  ```kotlin
  onNavigateToHubScan: (hubId: String) -> Unit = {},
  ```

- [ ] **Step 10.2: Gate the Hub Mode button on `state.isHubScanner`**

  In `HomeScreen.kt`, find the Hub Mode button block:
  ```kotlin
  Button(
      onClick = onNavigateToHubScan,
      ...
  ) {
      Text("🏭  Hub Mode", ...)
  }
  ```
  Wrap it in an `if` guard and pass `state.hubId`:
  ```kotlin
  if (state.isHubScanner) {
      Button(
          onClick = { onNavigateToHubScan(state.hubId) },
          modifier = Modifier
              .fillMaxWidth()
              .height(52.dp),
          shape  = RoundedCornerShape(14.dp),
          colors = ButtonDefaults.buttonColors(
              containerColor = Cyan.copy(alpha = 0.15f),
              contentColor   = Cyan,
          ),
      ) {
          Text("🏭  Hub Mode", fontWeight = FontWeight.Bold, fontSize = 15.sp)
      }
  }
  ```

- [ ] **Step 10.3: Update the lambda in `ShiftNavGraph.kt`**

  In `ShiftNavGraph.kt`, find:
  ```kotlin
  onNavigateToHubScan = { shiftNavController.navigateToHubScan() },
  ```
  Replace with:
  ```kotlin
  onNavigateToHubScan = { hubId -> shiftNavController.navigateToHubScan(hubId = hubId) },
  ```

- [ ] **Step 10.4: Run hub unit tests**

  ```
  ./gradlew :feature:hub:testDebugUnitTest :feature:home:testDebugUnitTest --no-daemon 2>&1 | tail -15
  ```
  Expected: BUILD SUCCESSFUL, all tests pass.

- [ ] **Step 10.5: Commit**

  ```
  git add apps/driver-app-android/feature/home/src/main/kotlin/io/logisticos/driver/feature/home/ui/HomeScreen.kt
  git add apps/driver-app-android/app/src/main/kotlin/io/logisticos/driver/navigation/ShiftNavGraph.kt
  git commit -m "feat(android): gate Hub Mode button on isHubScanner; pass hub_id to HubScanScreen"
  ```

---

## ── ADMIN PORTAL ────────────────────────────────────────────────────────────

## Task 11: Admin Portal — `hub-staff.ts` API utility

**Files:**
- Create: `apps/admin-portal/src/lib/api/hub-staff.ts`

- [ ] **Step 11.1: Create the hub-staff API utility**

  Create `apps/admin-portal/src/lib/api/hub-staff.ts`:
  ```typescript
  import { createApiClient } from './client';

  // ── Types ─────────────────────────────────────────────────────────────────────

  export interface HubDriver {
    id:         string;
    user_id:    string;
    first_name: string;
    last_name:  string;
    phone:      string;
    status:     string;
    hub_id:     string | null;
  }

  // ── API factory ───────────────────────────────────────────────────────────────

  export function createHubStaffApi() {
    const http = createApiClient();

    /** Drivers currently assigned to a specific hub (hub_id = hubId). */
    async function listHubScanners(hubId: string): Promise<HubDriver[]> {
      const res = await http.get<{ data: HubDriver[] }>(`/v1/drivers?hub_id=${encodeURIComponent(hubId)}`);
      return res.data.data;
    }

    /** Search all drivers in the tenant by name or phone fragment. */
    async function searchDrivers(query: string): Promise<HubDriver[]> {
      const res = await http.get<{ data: HubDriver[] }>(`/v1/drivers?search=${encodeURIComponent(query)}`);
      return res.data.data;
    }

    /**
     * Assign a driver as a hub scanner for the given hub.
     *
     * Identity-first dual-write with rollback:
     *   1. Grant hub_scanner role (identity) — if this fails, nothing is written.
     *   2. Set hub_id on driver-ops — if this fails, identity role is revoked and an
     *      error is thrown to surface the partial failure to the caller.
     */
    async function assignHubScanner(
      driverId: string,
      userId:   string,
      hubId:    string,
    ): Promise<void> {
      // Step 1 — identity (security gate)
      await http.patch(`/v1/users/${userId}/roles`, {
        role:   'hub_scanner',
        action: 'assign',
      });
      // Step 2 — operational data
      try {
        await http.patch(`/v1/drivers/${driverId}`, { hub_id: hubId });
      } catch (err) {
        // Rollback: revoke the identity role so the driver doesn't have a hub_id
        // without the security role, which would cause 403s on every scan.
        try {
          await http.patch(`/v1/users/${userId}/roles`, {
            role:   'hub_scanner',
            action: 'revoke',
          });
        } catch {
          // Rollback failed — log for ops visibility but re-throw original error
          console.error('[hub-staff] Role rollback failed after driver-ops write failure');
        }
        throw err;
      }
    }

    /**
     * Remove a hub scanner assignment.
     *
     * Identity-first: revoke the security role first so the driver loses scan
     * access immediately. Then clear hub_id from driver-ops. If the second call
     * fails, the role is re-assigned (rollback) to prevent the driver from being
     * stuck with hub_id but no role.
     */
    async function removeHubScanner(
      driverId: string,
      userId:   string,
    ): Promise<void> {
      // Step 1 — revoke security role first
      await http.patch(`/v1/users/${userId}/roles`, {
        role:   'hub_scanner',
        action: 'revoke',
      });
      // Step 2 — clear hub_id
      try {
        await http.patch(`/v1/drivers/${driverId}`, { remove_hub_id: true });
      } catch (err) {
        // Rollback: re-grant the role so the driver isn't left in a broken state
        try {
          await http.patch(`/v1/users/${userId}/roles`, {
            role:   'hub_scanner',
            action: 'assign',
          });
        } catch {
          console.error('[hub-staff] Role rollback failed after driver-ops clear failure');
        }
        throw err;
      }
    }

    return { listHubScanners, searchDrivers, assignHubScanner, removeHubScanner };
  }
  ```

- [ ] **Step 11.2: TypeScript check**

  From `apps/admin-portal`:
  ```
  npx tsc --noEmit 2>&1 | head -20
  ```
  Expected: no errors in hub-staff.ts.

- [ ] **Step 11.3: Commit**

  ```
  git add apps/admin-portal/src/lib/api/hub-staff.ts
  git commit -m "feat(admin-portal): hub-staff.ts API utility with identity-first dual-write"
  ```

---

## Task 12: Admin Portal — `HubStaffTab.tsx` component

**Files:**
- Create: `apps/admin-portal/src/app/(dashboard)/hubs/[hubId]/HubStaffTab.tsx`

- [ ] **Step 12.1: Create `HubStaffTab.tsx`**

  Create `apps/admin-portal/src/app/(dashboard)/hubs/[hubId]/HubStaffTab.tsx`:
  ```tsx
  'use client';

  import { useCallback, useEffect, useState } from 'react';
  import { motion } from 'framer-motion';
  import { UserCheck, UserMinus, Search, X } from 'lucide-react';
  import { GlassCard } from '@/components/ui/glass-card';
  import { variants } from '@/lib/design-system/tokens';
  import { createHubStaffApi, type HubDriver } from '@/lib/api/hub-staff';

  interface Props {
    hubId: string;
  }

  export default function HubStaffTab({ hubId }: Props) {
    const [scanners,    setScanners]    = useState<HubDriver[]>([]);
    const [loading,     setLoading]     = useState(true);
    const [actionError, setActionError] = useState<string | null>(null);

    // Search modal state
    const [showSearch,   setShowSearch]   = useState(false);
    const [searchQuery,  setSearchQuery]  = useState('');
    const [searchResult, setSearchResult] = useState<HubDriver[]>([]);
    const [searching,    setSearching]    = useState(false);
    const [assigning,    setAssigning]    = useState<string | null>(null); // driverId being assigned
    const [removing,     setRemoving]     = useState<string | null>(null); // driverId being removed

    const api = createHubStaffApi();

    const load = useCallback(async () => {
      setLoading(true);
      try {
        setScanners(await api.listHubScanners(hubId));
      } catch {
        setScanners([]);
      } finally {
        setLoading(false);
      }
    // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [hubId]);

    useEffect(() => { load(); }, [load]);

    async function handleSearch(q: string) {
      setSearchQuery(q);
      if (q.length < 2) { setSearchResult([]); return; }
      setSearching(true);
      try {
        const results = await api.searchDrivers(q);
        // Exclude already-assigned scanners from results
        const assignedIds = new Set(scanners.map(s => s.id));
        setSearchResult(results.filter(d => !assignedIds.has(d.id)));
      } catch {
        setSearchResult([]);
      } finally {
        setSearching(false);
      }
    }

    async function handleAssign(driver: HubDriver) {
      setAssigning(driver.id);
      setActionError(null);
      try {
        await api.assignHubScanner(driver.id, driver.user_id, hubId);
        setShowSearch(false);
        setSearchQuery('');
        setSearchResult([]);
        await load();
      } catch (err: unknown) {
        setActionError((err as { message?: string })?.message ?? 'Assignment failed');
      } finally {
        setAssigning(null);
      }
    }

    async function handleRemove(driver: HubDriver) {
      setRemoving(driver.id);
      setActionError(null);
      try {
        await api.removeHubScanner(driver.id, driver.user_id);
        await load();
      } catch (err: unknown) {
        setActionError((err as { message?: string })?.message ?? 'Removal failed');
      } finally {
        setRemoving(null);
      }
    }

    return (
      <motion.div
        key="hub-staff"
        variants={variants.staggerContainer}
        initial="hidden"
        animate="visible"
        className="h-full overflow-y-auto space-y-4 pb-6"
      >
        {/* Header row */}
        <motion.div variants={variants.fadeInUp} className="flex items-center justify-between">
          <div>
            <h2 className="font-heading text-base font-semibold text-white">Hub Scanners</h2>
            <p className="text-xs font-mono text-white/40 mt-0.5">
              Drivers assigned to this hub with hub_scanner role
            </p>
          </div>
          <button
            onClick={() => setShowSearch(true)}
            className="flex items-center gap-2 rounded-lg border border-cyan-neon/30 bg-cyan-neon/5 px-3 py-1.5 text-xs font-mono text-cyan-neon hover:bg-cyan-neon/10 transition-all"
          >
            <UserCheck size={12} /> Assign Driver
          </button>
        </motion.div>

        {/* Error banner */}
        {actionError && (
          <div className="rounded-lg border border-red-signal/30 bg-red-signal/10 px-4 py-2 text-xs font-mono text-red-signal flex items-center justify-between">
            {actionError}
            <button onClick={() => setActionError(null)}><X size={12} /></button>
          </div>
        )}

        {/* Scanners table */}
        <motion.div variants={variants.fadeInUp}>
          <GlassCard>
            {loading ? (
              <div className="flex items-center justify-center py-12">
                <div className="h-6 w-6 animate-spin rounded-full border-2 border-cyan-neon border-t-transparent" />
              </div>
            ) : scanners.length === 0 ? (
              <p className="py-10 text-center text-sm font-mono text-white/30">
                No hub scanners assigned yet
              </p>
            ) : (
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-b border-white/[0.06] text-left">
                    <th className="pb-2 pr-4 text-xs font-mono font-medium text-white/40">Name</th>
                    <th className="pb-2 pr-4 text-xs font-mono font-medium text-white/40">Phone</th>
                    <th className="pb-2 pr-4 text-xs font-mono font-medium text-white/40">Status</th>
                    <th className="pb-2 text-xs font-mono font-medium text-white/40" />
                  </tr>
                </thead>
                <tbody>
                  {scanners.map((d) => (
                    <tr key={d.id} className="border-b border-white/[0.03] last:border-0">
                      <td className="py-2.5 pr-4 font-medium text-white">
                        {d.first_name} {d.last_name}
                      </td>
                      <td className="py-2.5 pr-4 font-mono text-xs text-white/60">{d.phone}</td>
                      <td className="py-2.5 pr-4">
                        <span className={`rounded-full px-2 py-0.5 text-2xs font-mono font-medium ${
                          d.status === 'available' ? 'bg-green-signal/15 text-green-signal' :
                          d.status === 'offline'   ? 'bg-white/5 text-white/40' :
                                                     'bg-cyan-neon/10 text-cyan-neon'
                        }`}>
                          {d.status}
                        </span>
                      </td>
                      <td className="py-2.5 text-right">
                        <button
                          onClick={() => handleRemove(d)}
                          disabled={removing === d.id}
                          className="flex items-center gap-1.5 rounded-md border border-red-signal/20 bg-red-signal/5 px-2.5 py-1 text-2xs font-mono text-red-signal hover:bg-red-signal/10 disabled:opacity-40 transition-all ml-auto"
                        >
                          {removing === d.id
                            ? <span className="h-3 w-3 animate-spin rounded-full border border-red-signal border-t-transparent" />
                            : <UserMinus size={11} />}
                          Remove
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </GlassCard>
        </motion.div>

        {/* Search / Assign modal */}
        {showSearch && (
          <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
            <div className="w-full max-w-md rounded-2xl border border-white/[0.08] bg-[#0A0E1A] p-5 shadow-2xl">
              <div className="mb-4 flex items-center justify-between">
                <h3 className="font-heading text-sm font-semibold text-white">Assign Hub Scanner</h3>
                <button
                  onClick={() => { setShowSearch(false); setSearchQuery(''); setSearchResult([]); }}
                  className="text-white/40 hover:text-white/70"
                >
                  <X size={16} />
                </button>
              </div>
              <div className="relative mb-3">
                <Search size={13} className="absolute left-3 top-1/2 -translate-y-1/2 text-white/30" />
                <input
                  autoFocus
                  type="text"
                  placeholder="Search by name or phone…"
                  value={searchQuery}
                  onChange={(e) => handleSearch(e.target.value)}
                  className="w-full rounded-lg border border-white/[0.1] bg-white/[0.05] pl-8 pr-3 py-2 text-xs font-mono text-white placeholder:text-white/30 focus:border-cyan-neon/40 focus:outline-none"
                />
              </div>
              <div className="max-h-56 overflow-y-auto space-y-1">
                {searching && (
                  <p className="py-4 text-center text-xs font-mono text-white/40">Searching…</p>
                )}
                {!searching && searchResult.length === 0 && searchQuery.length >= 2 && (
                  <p className="py-4 text-center text-xs font-mono text-white/40">No results</p>
                )}
                {searchResult.map((d) => (
                  <button
                    key={d.id}
                    onClick={() => handleAssign(d)}
                    disabled={assigning === d.id}
                    className="w-full flex items-center justify-between rounded-lg border border-white/[0.05] bg-white/[0.02] px-3 py-2 text-left hover:bg-white/[0.05] disabled:opacity-40 transition-all"
                  >
                    <div>
                      <p className="text-xs font-medium text-white">{d.first_name} {d.last_name}</p>
                      <p className="text-2xs font-mono text-white/40">{d.phone}</p>
                    </div>
                    {assigning === d.id
                      ? <span className="h-4 w-4 animate-spin rounded-full border border-cyan-neon border-t-transparent" />
                      : <UserCheck size={13} className="text-cyan-neon" />}
                  </button>
                ))}
              </div>
            </div>
          </div>
        )}
      </motion.div>
    );
  }
  ```

- [ ] **Step 12.2: TypeScript check**

  ```
  npx tsc --noEmit 2>&1 | head -20
  ```
  Expected: no errors in HubStaffTab.tsx.

- [ ] **Step 12.3: Commit**

  ```
  git add apps/admin-portal/src/app/\(dashboard\)/hubs/\[hubId\]/HubStaffTab.tsx
  git commit -m "feat(admin-portal): HubStaffTab component with assign/remove hub scanner UI"
  ```

---

## Task 13: Admin Portal — add `hub-staff` tab to `HubDetailClient.tsx`

**Files:**
- Modify: `apps/admin-portal/src/app/(dashboard)/hubs/[hubId]/HubDetailClient.tsx`

- [ ] **Step 13.1: Add `'hub-staff'` to `HUB_TABS` and `TAB_LABELS`**

  In `HubDetailClient.tsx`, replace:
  ```typescript
  const HUB_TABS = ['overview', 'plan-load'] as const;
  type HubTab = (typeof HUB_TABS)[number];

  const TAB_LABELS: Record<HubTab, string> = {
    'overview':   'Overview',
    'plan-load':  'Plan Load (3D)',
  };
  ```
  With:
  ```typescript
  const HUB_TABS = ['overview', 'plan-load', 'hub-staff'] as const;
  type HubTab = (typeof HUB_TABS)[number];

  const TAB_LABELS: Record<HubTab, string> = {
    'overview':   'Overview',
    'plan-load':  'Plan Load (3D)',
    'hub-staff':  'Hub Staff',
  };
  ```

- [ ] **Step 13.2: Add `HubStaffTab` import**

  After the `ConsolidationPageClient` import add:
  ```typescript
  import HubStaffTab from './HubStaffTab';
  ```

- [ ] **Step 13.3: Add `Users` icon import from lucide-react**

  In the lucide import block, add `Users`:
  ```typescript
  import {
    Building2, ChevronLeft, MapPin, Layers,
    Package, Boxes, AlertTriangle, RefreshCw,
    FileText, ArrowRight, Users,
  } from 'lucide-react';
  ```

- [ ] **Step 13.4: Add icon for `hub-staff` tab in the tab bar**

  Find the tab button icon block:
  ```tsx
  {tab === 'plan-load' && <Boxes size={13} />}
  {tab === 'overview'  && <Package size={13} />}
  ```
  Add:
  ```tsx
  {tab === 'plan-load'  && <Boxes   size={13} />}
  {tab === 'overview'   && <Package size={13} />}
  {tab === 'hub-staff'  && <Users   size={13} />}
  ```

- [ ] **Step 13.5: Update `switchTab` URL logic**

  Replace:
  ```typescript
  function switchTab(tab: HubTab) {
    setActiveTab(tab);
    const url = tab === 'overview' ? `/hubs/${hubId}` : `/hubs/${hubId}?tab=plan-load`;
    router.replace(url, { scroll: false });
  }
  ```
  With:
  ```typescript
  function switchTab(tab: HubTab) {
    setActiveTab(tab);
    const url = tab === 'overview'
      ? `/hubs/${hubId}`
      : `/hubs/${hubId}?tab=${tab}`;
    router.replace(url, { scroll: false });
  }
  ```

- [ ] **Step 13.6: Add `hub-staff` tab content in `AnimatePresence`**

  After the `plan-load` tab block (before `</AnimatePresence>`), add:
  ```tsx
  {activeTab === 'hub-staff' && (
    <motion.div
      key="hub-staff-tab"
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={{ duration: 0.15 }}
      className="h-full overflow-y-auto"
    >
      <HubStaffTab hubId={hubId} />
    </motion.div>
  )}
  ```

- [ ] **Step 13.7: TypeScript check + build**

  ```
  npx tsc --noEmit 2>&1 | head -20
  ```
  Expected: no errors.

- [ ] **Step 13.8: Commit**

  ```
  git add apps/admin-portal/src/app/\(dashboard\)/hubs/\[hubId\]/HubDetailClient.tsx
  git commit -m "feat(admin-portal): Hub Staff tab on Hub Detail page"
  ```

---

## Task 14: Push + CI verification

- [ ] **Step 14.1: Push all commits**

  ```
  git push origin master
  ```

- [ ] **Step 14.2: Monitor CI**

  ```
  gh run list --limit 6
  ```
  Wait for `Build Android Driver App`, `CI — Rust Quickcheck`, and `CI — Frontend` to show `completed / success`.

---

## Self-Review Checklist

- [x] **Spec coverage — mid-shift staleness**: Task 9 adds `refreshHubProfile()` called in `init{}` + hooked into `syncShift()` (fires on every foreground). ✓
- [x] **Spec coverage — dual-write safety**: Task 11 `hub-staff.ts` — identity FIRST in both assign and remove; rollback on step-2 failure. ✓
- [x] **Spec coverage — hub_scanner RBAC**: Task 1 adds role with `SHIPMENT_READ + SHIPMENT_UPDATE` — covers both `record_scan_handler` and `shipment_by_awb_handler` permission requirements. ✓
- [x] **Spec coverage — GET /v1/drivers/me before /:id**: Task 5 Step 5.3 explicitly notes route order. ✓
- [x] **Type consistency**: `DriverProfileResponse` defined Task 7, used Task 8/9. `isHubScanner`/`hubId` defined Task 9, used Task 10. `HubDriver` defined Task 11, used Task 12. All consistent. ✓
- [x] **No placeholders**: all code blocks are complete and compilable. ✓
- [x] **hub_id in DriverDto**: Task 5 Step 5.1 adds it so the Android app can read it from `/drivers/me`. ✓
- [x] **driver_service register() hub_id**: Task 3 Step 3.7 sets `hub_id: None` in the new Driver literal. ✓
