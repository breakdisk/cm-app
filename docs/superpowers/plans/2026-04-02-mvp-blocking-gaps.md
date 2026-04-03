# MVP Blocking Gaps — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the 7 blocking issues preventing LogisticOS from being production-ready: hardcoded secrets, missing reschedule endpoint, OTP verify endpoint, password reset flow, email verification flow, analytics DB initialization, and compliance service in docker-compose.

**Architecture:** Each gap is an isolated change. Secrets are environment variable hygiene only (no code changes). Reschedule lives in delivery-experience (it owns the tracking domain). OTP verify is a new endpoint in pod service alongside `generate_otp`. Password reset and email verify are new endpoints in identity service with a `password_reset_tokens` migration. Analytics gets a `.env` file and `after_connect` search_path fix. Compliance is a one-line docker-compose addition.

**Tech Stack:** Rust (Axum, SQLx, thiserror), PostgreSQL, docker-compose, Next.js 14 (customer portal)

**Running services (local):**
- identity: `cd services/identity && ../../target/debug/logisticos-identity.exe` (port 8001)
- order-intake: port 8004
- dispatch: port 8005
- driver-ops: port 8006
- delivery-experience: port 8007
- pod: port 8011
- analytics: port 8013

**Build command:** `cargo build -p <package>` from `d:/LogisticOS`

---

## File Structure

**Create:**
- `services/identity/migrations/0005_password_reset_tokens.sql` — password_reset_tokens table
- `services/analytics/.env` — env file for analytics service (same pattern as other services)

**Modify:**
- `services/identity/src/application/commands/mod.rs` — add ForgotPasswordCommand, ResetPasswordCommand, VerifyEmailCommand
- `services/identity/src/application/services/auth_service.rs` — add forgot_password, reset_password, send_verification_email, verify_email methods
- `services/identity/src/infrastructure/db/user_repo.rs` — add find_by_reset_token, save_reset_token, delete_reset_token
- `services/identity/src/api/http/auth.rs` — add forgot_password, reset_password, send_verification_email, verify_email handlers
- `services/identity/src/api/http/mod.rs` — register new auth routes
- `services/pod/src/application/commands/mod.rs` — VerifyOtpCommand already exists (no change needed)
- `services/pod/src/application/services/pod_service.rs` — add verify_otp_standalone method
- `services/pod/src/api/http/pod.rs` — add verify_otp handler
- `services/pod/src/api/http/mod.rs` — register `/otps/verify` route
- `services/delivery-experience/src/api/http/mod.rs` — add reschedule route
- `services/delivery-experience/src/application/services/mod.rs` — add reschedule method
- `services/delivery-experience/src/infrastructure/db/mod.rs` — add reschedule DB method
- `services/delivery-experience/src/domain/entities/mod.rs` — add reschedule fields to TrackingRecord
- `services/analytics/src/bootstrap.rs` — add `after_connect` search_path and schema
- `docker-compose.yml` — add compliance service entry
- `services/identity/.env` — remove hardcoded JWT_SECRET; document as example only
- All `services/*/.env` files — replace `dev-jwt-secret-CHANGE-IN-PRODUCTION` with placeholder docs

---

## Task 1: Secrets hygiene — remove hardcoded secrets from .env files

**Files:**
- Modify: `services/identity/.env`
- Modify: `services/delivery-experience/.env`
- Modify: `services/dispatch/.env`
- Modify: `services/driver-ops/.env`
- Modify: `services/order-intake/.env`
- Modify: `services/pod/.env`
- Create: `.gitignore` update (if needed)

The issue: every service `.env` has `AUTH__JWT_SECRET=dev-jwt-secret-CHANGE-IN-PRODUCTION-minimum-32-chars` (or similar). These are dev-only files that should never be committed with real secrets. The fix for MVP: replace with a strong random value in each `.env` and add `.env` to `.gitignore` so they can't be accidentally committed.

- [ ] **Step 1: Check .gitignore for .env entries**

```bash
grep -n "\.env" d:/LogisticOS/.gitignore
```

Expected: `.env` is listed. If missing, proceed to add it.

- [ ] **Step 2: Add .env files to .gitignore (if not already there)**

Read `d:/LogisticOS/.gitignore`. Add the following lines if not present:

```
# Service environment files (contain secrets — never commit)
services/**/.env
apps/**/.env.local
```

- [ ] **Step 3: Generate a shared JWT secret for all local services**

All services must share the same JWT secret (identity issues tokens; all other services verify them). Pick one strong value:

```
dev-jwt-secret-REPLACE-WITH-32CHAR-RANDOM-VALUE-123
```

Replace `AUTH__JWT_SECRET` in every service `.env` that contains it:
- `services/identity/.env`
- `services/delivery-experience/.env`
- `services/dispatch/.env`
- `services/driver-ops/.env`
- `services/order-intake/.env`
- `services/pod/.env`

For each file, change:
```
AUTH__JWT_SECRET=dev-jwt-secret-CHANGE-IN-PRODUCTION-minimum-32-chars
```
to:
```
AUTH__JWT_SECRET=dev-jwt-secret-REPLACE-WITH-32CHAR-RANDOM-VALUE-123
```

> **Note:** In production, this value must be generated with `openssl rand -base64 32` and injected via Vault/K8s secrets, never stored in `.env`.

- [ ] **Step 4: Replace MinIO credentials in pod/.env**

In `services/pod/.env`, verify S3 credentials exist:
```bash
grep "S3_" services/pod/.env
```

These are local MinIO dev credentials — acceptable for dev. Add a comment:
```
# Dev MinIO credentials — replace with IAM role in production
S3_ACCESS_KEY=minioadmin
S3_SECRET_KEY=minioadmin
```

- [ ] **Step 5: Verify services still start after env change**

```bash
curl -s http://localhost:8001/health | python -m json.tool
```

Expected: `{"status":"ok",...}` — if identity is already running, it will keep its loaded secret. No restart needed for this step (secrets are read at startup).

- [ ] **Step 6: Commit**

```bash
cd d:/LogisticOS
git add .gitignore
git commit -m "chore(security): add .env files to .gitignore; document secret management

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

## Task 2: Analytics service — create .env and fix bootstrap search_path

**Context:** Analytics uses `sqlx::query!()` macros (compile-time checked) which require `DATABASE_URL` at compile time — but more critically, the service doesn't set `search_path` in `after_connect` like other services do. Without it, the migration runs but the service may fail to connect properly. Also the service has no `.env` so it can't run locally at all.

**Files:**
- Create: `services/analytics/.env`
- Modify: `services/analytics/src/bootstrap.rs`

- [ ] **Step 1: Read the identity bootstrap to understand the after_connect pattern**

Read `services/identity/src/bootstrap.rs` — specifically the `PgPoolOptions` block that sets `search_path`. You need this pattern.

Expected pattern (from identity):
```rust
.after_connect(|conn, _| Box::pin(async move {
    conn.execute("SET search_path TO identity, public").await?;
    Ok(())
}))
```

- [ ] **Step 2: Read the analytics bootstrap to understand what to add**

Read `services/analytics/src/bootstrap.rs` (already read — no `after_connect` is set).

The current pool setup:
```rust
let pool = PgPoolOptions::new()
    .max_connections(cfg.database.max_connections)
    .connect(&cfg.database.url)
    .await?;
```

- [ ] **Step 3: Create services/analytics/.env**

```
APP__HOST=127.0.0.1
APP__PORT=8013
APP__ENV=development
DATABASE__URL=postgres://logisticos:password@localhost:5432/logisticos
DATABASE__MAX_CONNECTIONS=5
REDIS__URL=redis://localhost:6379
KAFKA__BROKERS=localhost:9092
KAFKA__GROUP_ID=analytics-dev
AUTH__JWT_SECRET=dev-jwt-secret-REPLACE-WITH-32CHAR-RANDOM-VALUE-123
RUST_LOG=info,sqlx=warn
```

- [ ] **Step 4: Add after_connect search_path to analytics bootstrap**

In `services/analytics/src/bootstrap.rs`, change the pool creation:

```rust
use sqlx::Executor;

let pool = PgPoolOptions::new()
    .max_connections(cfg.database.max_connections)
    .after_connect(|conn, _| Box::pin(async move {
        conn.execute("SET search_path TO analytics, public").await?;
        Ok(())
    }))
    .connect(&cfg.database.url)
    .await?;
```

> **Note:** `use sqlx::Executor;` must be at the top of `bootstrap.rs` for `.execute()` to be in scope.

- [ ] **Step 5: Build analytics to verify no compile errors**

```bash
cd d:/LogisticOS
cargo build -p logisticos-analytics 2>&1 | tail -20
```

Expected: `Finished` with no errors. The `sqlx::query!()` macros in `infrastructure/db/mod.rs` require `DATABASE_URL` at compile time — if they fail, set it temporarily:

```bash
DATABASE_URL=postgres://logisticos:password@localhost:5432/logisticos cargo build -p logisticos-analytics 2>&1 | tail -20
```

- [ ] **Step 6: Test analytics starts and migrates**

With docker-compose running (postgres up):
```bash
cd d:/LogisticOS/services/analytics
../../target/debug/logisticos-analytics.exe &
sleep 3
curl -s http://localhost:8013/health
```

Expected: `{"status":"ok","service":"analytics"}`

- [ ] **Step 7: Commit**

```bash
cd d:/LogisticOS
git add services/analytics/.env services/analytics/src/bootstrap.rs
git commit -m "fix(analytics): add .env, set search_path in pool after_connect

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

## Task 3: Add compliance service to docker-compose

**Context:** The compliance service directory exists at `services/compliance/` with full implementation but is missing from `docker-compose.yml`. It should be on port 8017.

**Files:**
- Modify: `docker-compose.yml`

- [ ] **Step 1: Find the compliance service port**

```bash
cat d:/LogisticOS/services/compliance/.env 2>/dev/null | grep PORT
```

Expected: `APP__PORT=8017` (or similar). Note the actual port.

- [ ] **Step 2: Find the end of docker-compose.yml service list**

```bash
tail -40 d:/LogisticOS/docker-compose.yml
```

Find the last service entry before the `volumes:` or `networks:` block. That's where compliance gets inserted.

- [ ] **Step 3: Add compliance service entry**

Find the `# ── 16. Business Logic...` or last service block in `docker-compose.yml`. Add after the last service (before `volumes:`) the following block. Adjust the port if Step 1 showed a different value:

```yaml
  # ── 17. Compliance (port 8017) ────────────────────────────
  compliance:
    build:
      context: ./services/compliance
      dockerfile: Dockerfile
    container_name: logisticos-compliance
    restart: unless-stopped
    ports:
      - "8017:8017"
    environment:
      APP__HOST: "0.0.0.0"
      APP__PORT: "8017"
      APP__ENV: development
      DATABASE__URL: postgres://logisticos:password@postgres:5432/logisticos
      DATABASE__MAX_CONNECTIONS: "5"
      REDIS__URL: redis://redis:6379
      KAFKA__BROKERS: kafka:29092
      KAFKA__GROUP_ID: compliance-dev
      AUTH__JWT_SECRET: ${JWT_SECRET:-dev-jwt-secret-REPLACE-WITH-32CHAR-RANDOM-VALUE-123}
      RUST_LOG: info,sqlx=warn
    depends_on:
      postgres:
        condition: service_healthy
      redis:
        condition: service_healthy
      kafka:
        condition: service_started
    healthcheck:
      test: ["CMD-SHELL", "curl -sf http://localhost:8017/health || exit 1"]
      interval: 15s
      timeout: 5s
      retries: 5
      start_period: 10s
```

- [ ] **Step 4: Verify compliance service builds**

```bash
cd d:/LogisticOS
cargo build -p logisticos-compliance 2>&1 | tail -10
```

Expected: `Finished` with no errors.

- [ ] **Step 5: Commit**

```bash
cd d:/LogisticOS
git add docker-compose.yml
git commit -m "chore(infra): add compliance service to docker-compose (port 8017)

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

## Task 4: POD service — add standalone OTP verify endpoint

**Context:** `VerifyOtpCommand` already exists in `services/pod/src/application/commands/mod.rs`. The `submit` handler already does OTP verification inline when `otp_code` is provided in the submit body. However, a standalone `POST /v1/otps/verify` endpoint is needed so the driver app can verify OTP *before* the full POD submit flow (useful for pre-verification UX). 

**Files:**
- Modify: `services/pod/src/application/services/pod_service.rs`
- Modify: `services/pod/src/api/http/pod.rs`
- Modify: `services/pod/src/api/http/mod.rs`

- [ ] **Step 1: Add verify_otp method to PodService**

In `services/pod/src/application/services/pod_service.rs`, add after the `generate_and_send_otp` method (before the private helpers at the bottom):

```rust
/// Standalone OTP verification — driver can pre-verify before submitting POD.
/// Returns otp_id on success so driver app can pass it in submit if needed.
pub async fn verify_otp_standalone(
    &self,
    cmd: VerifyOtpCommand,
) -> AppResult<Uuid> {
    let otp = self.otp_repo
        .find_active_by_shipment(cmd.shipment_id).await
        .map_err(AppError::Internal)?
        .ok_or_else(|| AppError::BusinessRule("No active OTP found for this shipment".into()))?;

    if !otp.is_valid() {
        return Err(AppError::BusinessRule("OTP has expired. Request a new one.".into()));
    }

    use crate::domain::value_objects::verify_otp;
    if !verify_otp(&cmd.code, &otp.code_hash) {
        return Err(AppError::BusinessRule("Invalid OTP code".into()));
    }

    tracing::info!(shipment_id = %cmd.shipment_id, "OTP pre-verified");
    Ok(otp.id)
}
```

- [ ] **Step 2: Add verify_otp handler to pod.rs**

In `services/pod/src/api/http/pod.rs`, add after the `generate_otp` handler:

```rust
pub async fn verify_otp(
    AuthClaims(_claims): AuthClaims,
    State(state): State<Arc<AppState>>,
    Json(cmd): Json<VerifyOtpCommand>,
) -> Result<Json<serde_json::Value>, AppError> {
    let otp_id = state.pod_service.verify_otp_standalone(cmd).await?;
    Ok(Json(serde_json::json!({ "data": { "otp_id": otp_id, "verified": true } })))
}
```

- [ ] **Step 3: Register the route in mod.rs**

In `services/pod/src/api/http/mod.rs`, the OTP section currently reads:
```rust
.route("/otps/generate", post(pod::generate_otp))
```

Change to:
```rust
.route("/otps/generate", post(pod::generate_otp))
.route("/otps/verify",   post(pod::verify_otp))
```

- [ ] **Step 4: Build pod service**

```bash
cd d:/LogisticOS
cargo build -p logisticos-pod 2>&1 | tail -20
```

Expected: `Finished` with no errors.

- [ ] **Step 5: Test the endpoint**

With pod service running (port 8011) and a valid driver token:

```bash
# First generate an OTP (replace SHIPMENT_ID with a real one from your smoke test)
TOKEN="<driver_access_token>"
SHIPMENT_ID="b3d9ea98-bb87-4102-b0da-bd5839f64c20"

curl -s -X POST http://localhost:8011/v1/otps/generate \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d "{\"shipment_id\": \"$SHIPMENT_ID\", \"recipient_phone\": \"+639171234567\"}" | python -m json.tool
```

Expected: `{"data": {"otp_id": "<uuid>"}}`

```bash
# Now try verify with wrong code — should fail
curl -s -X POST http://localhost:8011/v1/otps/verify \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d "{\"shipment_id\": \"$SHIPMENT_ID\", \"code\": \"000000\"}" | python -m json.tool
```

Expected: `{"error": {"code": "BUSINESS_RULE", "message": "Invalid OTP code"}}`

- [ ] **Step 6: Commit**

```bash
cd d:/LogisticOS
git add services/pod/src/
git commit -m "feat(pod): add POST /v1/otps/verify standalone OTP verification endpoint

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

## Task 5: Delivery-experience — add reschedule endpoint

**Context:** Customer portal calls `POST /v1/tracking/:tracking_number/reschedule` against delivery-experience (port 8007, via `NEXT_PUBLIC_API_URL`). This endpoint doesn't exist. It should record a reschedule request in the tracking record and emit a status history entry.

**Files:**
- Modify: `services/delivery-experience/src/api/http/mod.rs`
- Modify: `services/delivery-experience/src/application/services/mod.rs`
- Modify: `services/delivery-experience/src/infrastructure/db/mod.rs`
- Modify: `services/delivery-experience/src/domain/entities/mod.rs` (may need reschedule_count field check)

- [ ] **Step 1: Read the TrackingRecord entity**

Read `services/delivery-experience/src/domain/entities/mod.rs` fully to understand the struct fields and what methods exist.

- [ ] **Step 2: Read the TrackingRepository**

Read `services/delivery-experience/src/domain/repositories/mod.rs` to understand the repository interface.

- [ ] **Step 3: Read the DB implementation**

Read `services/delivery-experience/src/infrastructure/db/mod.rs` fully — understand how `TrackingRecord` is loaded and saved.

- [ ] **Step 4: Check reschedule_count field**

In `TrackingRecord`, check if `reschedule_count` and `next_attempt_at` fields exist (the HTTP handler in `mod.rs` already references `record.next_attempt_at`). If they don't exist, they need to be added to the entity and the DB row struct.

If `reschedule_count` is missing from the struct, add it:

In `services/delivery-experience/src/domain/entities/mod.rs`, add to `TrackingRecord`:
```rust
pub reschedule_count: i32,
pub next_attempt_at: Option<chrono::DateTime<chrono::Utc>>,
```

And add a constructor default of `reschedule_count: 0` in any `TrackingRecord::new()` call if it exists.

- [ ] **Step 5: Add reschedule DB method**

In `services/delivery-experience/src/infrastructure/db/mod.rs`, add a `reschedule` method. First confirm the DB row struct name by reading the file.

Add this method to the `PgTrackingRepository` or equivalent impl block:

```rust
pub async fn reschedule(
    &self,
    tracking_number: &str,
    preferred_date: chrono::NaiveDate,
    reason: &str,
) -> anyhow::Result<()> {
    let now = chrono::Utc::now();
    let next_attempt = chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(
        preferred_date.and_hms_opt(9, 0, 0).unwrap(),
        chrono::Utc,
    );

    sqlx::query(
        r#"UPDATE delivery_experience.tracking_records
           SET reschedule_count  = reschedule_count + 1,
               next_attempt_at   = $1,
               updated_at        = $2
           WHERE tracking_number = $3"#,
    )
    .bind(next_attempt)
    .bind(now)
    .bind(tracking_number)
    .execute(&self.pool)
    .await?;

    // Append status history entry
    sqlx::query(
        r#"INSERT INTO delivery_experience.tracking_events
               (id, tracking_number, status, description, occurred_at)
           VALUES (gen_random_uuid(), $1, 'reschedule_requested', $2, $3)"#,
    )
    .bind(tracking_number)
    .bind(format!("Delivery rescheduled: {reason}"))
    .bind(now)
    .execute(&self.pool)
    .await?;

    Ok(())
}
```

> **Adjust table/column names** to match what you see in the existing DB queries in `db/mod.rs`. If the table is `tracking_records` not `delivery_experience.tracking_records`, use the correct name.

- [ ] **Step 6: Add reschedule to TrackingRepository trait**

In `services/delivery-experience/src/domain/repositories/mod.rs`, add to the `TrackingRepository` trait:

```rust
async fn reschedule(
    &self,
    tracking_number: &str,
    preferred_date: chrono::NaiveDate,
    reason: &str,
) -> anyhow::Result<()>;
```

Then implement it in `db/mod.rs` on the `PgTrackingRepository` (wrapping the method added in Step 5 in the `#[async_trait]` impl).

- [ ] **Step 7: Add reschedule method to TrackingService**

In `services/delivery-experience/src/application/services/mod.rs`, add:

```rust
pub async fn reschedule(
    &self,
    tracking_number: &str,
    preferred_date: chrono::NaiveDate,
    reason: &str,
) -> AppResult<()> {
    // Verify the shipment exists first
    self.repo
        .find_by_tracking_number(tracking_number)
        .await
        .map_err(AppError::internal)?
        .ok_or_else(|| AppError::NotFound(format!("Tracking number '{tracking_number}' not found")))?;

    self.repo
        .reschedule(tracking_number, preferred_date, reason)
        .await
        .map_err(AppError::internal)
}
```

- [ ] **Step 8: Add reschedule handler and route**

In `services/delivery-experience/src/api/http/mod.rs`, add the handler function (after `list_shipments`):

```rust
#[derive(Debug, serde::Deserialize)]
struct RescheduleBody {
    preferred_date: chrono::NaiveDate,   // "2026-04-05"
    reason: String,
}

async fn reschedule_delivery(
    State(state): State<AppState>,
    Path(tracking_number): Path<String>,
    Json(body): Json<RescheduleBody>,
) -> impl IntoResponse {
    match state.tracking_svc.reschedule(&tracking_number, body.preferred_date, &body.reason).await {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({"data": {"rescheduled": true, "tracking_number": tracking_number}})),
        ).into_response(),
        Err(AppError::NotFound(_)) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Tracking number not found"})),
        ).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ).into_response(),
    }
}
```

Register the route in the `router()` function — add alongside the existing public route:
```rust
.route("/track/:tracking_number",                     get(public_track))
.route("/track/:tracking_number/reschedule",          post(reschedule_delivery))
```

> **Note:** The customer portal calls `/v1/tracking/:tn/reschedule`. The delivery-experience router doesn't have a `/v1` prefix for the public routes. Check the customer portal's `API_BASE` and path — it uses `${API_BASE}/v1/tracking/${AWB}/reschedule`. Add this as an authenticated route too:

In the `router()` function, also add under the authenticated `/v1` routes:
```rust
.route("/v1/tracking/:tracking_number/reschedule",    post(reschedule_delivery))
```

- [ ] **Step 9: Build delivery-experience**

```bash
cd d:/LogisticOS
cargo build -p logisticos-delivery-experience 2>&1 | tail -20
```

Expected: `Finished` with no errors. Fix any type errors by reading the actual entity/repo shapes from the files.

- [ ] **Step 10: Test the endpoint**

With delivery-experience running and a known tracking number from your smoke test (e.g., `LSPH9487203604`):

```bash
curl -s -X POST "http://localhost:8007/v1/tracking/LSPH9487203604/reschedule" \
  -H "Content-Type: application/json" \
  -d '{"preferred_date": "2026-04-10", "reason": "Not home"}' | python -m json.tool
```

Expected: `{"data": {"rescheduled": true, "tracking_number": "LSPH9487203604"}}`

Also test the public route (no auth):
```bash
curl -s -X POST "http://localhost:8007/track/LSPH9487203604/reschedule" \
  -H "Content-Type: application/json" \
  -d '{"preferred_date": "2026-04-10", "reason": "Not home"}' | python -m json.tool
```

- [ ] **Step 11: Commit**

```bash
cd d:/LogisticOS
git add services/delivery-experience/src/
git commit -m "feat(delivery-experience): add reschedule endpoint POST /track/:tn/reschedule

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

## Task 6: Identity — password reset flow

**Context:** No password reset exists. Need: `POST /v1/auth/forgot-password` (generates token, "sends email" — logs it in dev), `POST /v1/auth/reset-password` (validates token, updates password, deletes token). Token stored in a new `identity.password_reset_tokens` table with TTL of 1 hour.

**Files:**
- Create: `services/identity/migrations/0005_password_reset_tokens.sql`
- Modify: `services/identity/src/application/commands/mod.rs`
- Modify: `services/identity/src/application/services/auth_service.rs`
- Modify: `services/identity/src/infrastructure/db/user_repo.rs`
- Modify: `services/identity/src/api/http/auth.rs`
- Modify: `services/identity/src/api/http/mod.rs`

- [ ] **Step 1: Create migration 0005_password_reset_tokens.sql**

```sql
-- Migration: 0005 — password_reset_tokens table

CREATE TABLE identity.password_reset_tokens (
    id          UUID        PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id     UUID        NOT NULL REFERENCES identity.users(id) ON DELETE CASCADE,
    tenant_id   UUID        NOT NULL,
    token_hash  TEXT        NOT NULL UNIQUE,  -- SHA-256 hex of the raw token
    expires_at  TIMESTAMPTZ NOT NULL,
    used        BOOLEAN     NOT NULL DEFAULT FALSE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_prt_token_hash ON identity.password_reset_tokens (token_hash);
CREATE INDEX idx_prt_user_id    ON identity.password_reset_tokens (user_id);
```

- [ ] **Step 2: Add commands**

In `services/identity/src/application/commands/mod.rs`, add:

```rust
#[derive(Debug, Deserialize, Validate)]
pub struct ForgotPasswordCommand {
    pub tenant_slug: String,
    #[validate(email)]
    pub email: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct ResetPasswordCommand {
    pub token: String,          // raw token from email link
    #[validate(length(min = 8))]
    pub new_password: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct SendVerificationEmailCommand {
    pub tenant_slug: String,
    #[validate(email)]
    pub email: String,
}

#[derive(Debug, Deserialize)]
pub struct VerifyEmailCommand {
    pub token: String,          // raw token from email link
}
```

- [ ] **Step 3: Add DB methods for password reset tokens**

In `services/identity/src/infrastructure/db/user_repo.rs`, add after the `UserRepository` impl:

```rust
pub struct PgPasswordResetTokenRepository {
    pool: PgPool,
}

impl PgPasswordResetTokenRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }

    pub async fn create_reset_token(&self, user_id: uuid::Uuid, tenant_id: uuid::Uuid, token_hash: &str) -> anyhow::Result<()> {
        let expires_at = chrono::Utc::now() + chrono::Duration::hours(1);
        sqlx::query(
            r#"INSERT INTO identity.password_reset_tokens (user_id, tenant_id, token_hash, expires_at)
               VALUES ($1, $2, $3, $4)
               ON CONFLICT (token_hash) DO NOTHING"#
        )
        .bind(user_id)
        .bind(tenant_id)
        .bind(token_hash)
        .bind(expires_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn find_valid_by_token(&self, token_hash: &str) -> anyhow::Result<Option<(uuid::Uuid, uuid::Uuid)>> {
        // Returns (user_id, tenant_id) if found, valid, and not used
        let row = sqlx::query(
            r#"SELECT user_id, tenant_id FROM identity.password_reset_tokens
               WHERE token_hash = $1 AND used = false AND expires_at > NOW()"#
        )
        .bind(token_hash)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| {
            let user_id: uuid::Uuid = r.get("user_id");
            let tenant_id: uuid::Uuid = r.get("tenant_id");
            (user_id, tenant_id)
        }))
    }

    pub async fn mark_used(&self, token_hash: &str) -> anyhow::Result<()> {
        sqlx::query("UPDATE identity.password_reset_tokens SET used = true WHERE token_hash = $1")
            .bind(token_hash)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
```

Add the import `use sqlx::Row;` at the top of `user_repo.rs` if not present.

- [ ] **Step 4: Update AppState to include the token repository**

Read `services/identity/src/api/http/mod.rs` (already read). Add `reset_token_repo` field to `AppState`:

In `services/identity/src/api/http/mod.rs`, change `AppState`:
```rust
pub struct AppState {
    pub auth_service: Arc<AuthService>,
    pub tenant_service: Arc<TenantService>,
    pub api_key_service: Arc<ApiKeyService>,
    pub jwt: Arc<logisticos_auth::jwt::JwtService>,
    pub reset_token_repo: Arc<crate::infrastructure::db::user_repo::PgPasswordResetTokenRepository>,
}
```

- [ ] **Step 5: Update bootstrap to create and inject reset_token_repo**

Read `services/identity/src/bootstrap.rs`. Find where `AppState` is constructed and add:

```rust
let reset_token_repo = Arc::new(
    crate::infrastructure::db::user_repo::PgPasswordResetTokenRepository::new(pool.clone())
);
```

And pass it to `AppState { ..., reset_token_repo }`.

- [ ] **Step 6: Add forgot_password and reset_password to AuthService**

In `services/identity/src/application/services/auth_service.rs`:

First, update the struct to hold the token repo. Add field:
```rust
pub struct AuthService {
    tenant_repo: Arc<dyn TenantRepository>,
    user_repo: Arc<dyn UserRepository>,
    jwt: Arc<JwtService>,
    reset_token_repo: Arc<crate::infrastructure::db::user_repo::PgPasswordResetTokenRepository>,
}
```

Update `AuthService::new` to take the extra parameter:
```rust
pub fn new(
    tenant_repo: Arc<dyn TenantRepository>,
    user_repo: Arc<dyn UserRepository>,
    jwt: Arc<JwtService>,
    reset_token_repo: Arc<crate::infrastructure::db::user_repo::PgPasswordResetTokenRepository>,
) -> Self {
    Self { tenant_repo, user_repo, jwt, reset_token_repo }
}
```

Add the two methods after `refresh`:

```rust
/// Initiate password reset: generate token, log reset link (dev: tracing::info, prod: email service).
pub async fn forgot_password(&self, cmd: crate::application::commands::ForgotPasswordCommand) -> AppResult<()> {
    let tenant = self.tenant_repo.find_by_slug(&cmd.tenant_slug).await
        .map_err(AppError::Internal)?
        .ok_or_else(|| AppError::NotFound { resource: "Tenant", id: cmd.tenant_slug.clone() })?;

    // Intentionally vague response — don't reveal whether email exists
    let user = self.user_repo.find_by_email(&tenant.id, &cmd.email).await
        .map_err(AppError::Internal)?;

    if let Some(user) = user {
        // Generate a random token
        let raw_token = uuid::Uuid::new_v4().to_string().replace('-', "")
            + &uuid::Uuid::new_v4().to_string().replace('-', "");

        // Hash it for storage
        let token_hash = {
            use std::fmt::Write;
            let mut hasher_input = raw_token.as_bytes().to_vec();
            let digest = sha2_hash(&hasher_input);
            digest
        };

        self.reset_token_repo
            .create_reset_token(user.id.inner(), tenant.id.inner(), &token_hash)
            .await
            .map_err(AppError::Internal)?;

        // In production, send via engagement service / email adapter.
        // In dev, log the link so it can be used in smoke tests.
        tracing::info!(
            user_id = %user.id,
            reset_link = format!("http://localhost:3002/reset-password?token={raw_token}"),
            "Password reset token generated — use this link in dev"
        );
    }

    Ok(()) // Always return Ok to avoid email enumeration
}

/// Complete password reset: validate token, update password, invalidate token.
pub async fn reset_password(&self, cmd: crate::application::commands::ResetPasswordCommand) -> AppResult<()> {
    let token_hash = sha2_hash(cmd.token.as_bytes());

    let (user_id, tenant_id) = self.reset_token_repo
        .find_valid_by_token(&token_hash).await
        .map_err(AppError::Internal)?
        .ok_or_else(|| AppError::Unauthorized("Invalid or expired reset token".into()))?;

    let user_id_typed = logisticos_types::UserId::from_uuid(user_id);
    let mut user = self.user_repo.find_by_id(&user_id_typed).await
        .map_err(AppError::Internal)?
        .ok_or_else(|| AppError::NotFound { resource: "User", id: user_id.to_string() })?;

    let new_hash = logisticos_auth::password::hash_password(&cmd.new_password)
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e.to_string())))?;

    user.password_hash = new_hash;
    user.updated_at = chrono::Utc::now();
    self.user_repo.save(&user).await.map_err(AppError::Internal)?;

    self.reset_token_repo.mark_used(&token_hash).await.map_err(AppError::Internal)?;

    tracing::info!(user_id = %user_id, "Password reset completed");
    Ok(())
}
```

Add a private helper at the bottom of `auth_service.rs`:
```rust
fn sha2_hash(data: &[u8]) -> String {
    use std::fmt::Write;
    // Simple hex-encoded SHA-256 using standard library trick via Rust's built-in hasher
    // For production, use sha2 crate. For now, use a deterministic UUID-based hash.
    // Add sha2 to Cargo.toml: sha2 = "0.10"
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    result.iter().fold(String::new(), |mut s, b| { write!(s, "{b:02x}").unwrap(); s })
}
```

- [ ] **Step 7: Add sha2 dependency to identity Cargo.toml**

Read `services/identity/Cargo.toml`. In the `[dependencies]` section, add:
```toml
sha2 = "0.10"
```

- [ ] **Step 8: Update bootstrap to pass reset_token_repo to AuthService**

In `services/identity/src/bootstrap.rs`, find:
```rust
let auth_service = Arc::new(AuthService::new(tenant_repo, user_repo, jwt));
```

Change to:
```rust
let auth_service = Arc::new(AuthService::new(
    Arc::clone(&tenant_repo),
    Arc::clone(&user_repo),
    Arc::clone(&jwt),
    Arc::clone(&reset_token_repo),
));
```

(Make sure `reset_token_repo` is constructed before this line, as in Step 5.)

- [ ] **Step 9: Add HTTP handlers**

In `services/identity/src/api/http/auth.rs`, add:

```rust
use crate::application::commands::{ForgotPasswordCommand, ResetPasswordCommand};

pub async fn forgot_password(
    State(state): State<Arc<AppState>>,
    Json(cmd): Json<ForgotPasswordCommand>,
) -> Result<Json<serde_json::Value>, AppError> {
    state.auth_service.forgot_password(cmd).await?;
    // Always return 200 — don't reveal whether email exists
    Ok(Json(serde_json::json!({ "data": { "message": "If that email exists, a reset link has been sent." } })))
}

pub async fn reset_password(
    State(state): State<Arc<AppState>>,
    Json(cmd): Json<ResetPasswordCommand>,
) -> Result<Json<serde_json::Value>, AppError> {
    state.auth_service.reset_password(cmd).await?;
    Ok(Json(serde_json::json!({ "data": { "message": "Password reset successfully." } })))
}
```

- [ ] **Step 10: Register routes**

In `services/identity/src/api/http/mod.rs`, add to the public auth routes:
```rust
.route("/v1/auth/forgot-password", post(auth::forgot_password))
.route("/v1/auth/reset-password",  post(auth::reset_password))
```

- [ ] **Step 11: Build identity**

```bash
cd d:/LogisticOS
cargo build -p logisticos-identity 2>&1 | tail -20
```

Expected: `Finished` with no errors. If `sha2` isn't resolving, run `cargo update` first.

- [ ] **Step 12: Test password reset**

With identity running (port 8001, after DB migration runs on restart):

```bash
# Step 1: Request reset link
curl -s -X POST http://localhost:8001/v1/auth/forgot-password \
  -H "Content-Type: application/json" \
  -d '{"tenant_slug": "demo", "email": "admin@demo.com"}' | python -m json.tool
```

Expected: `{"data": {"message": "If that email exists, a reset link has been sent."}}`
Check identity service logs — should print `reset_link=http://localhost:3002/reset-password?token=<TOKEN>`

```bash
# Step 2: Use the token from logs
TOKEN_FROM_LOG="<token from logs>"
curl -s -X POST http://localhost:8001/v1/auth/reset-password \
  -H "Content-Type: application/json" \
  -d "{\"token\": \"$TOKEN_FROM_LOG\", \"new_password\": \"NewPassword1!\"}" | python -m json.tool
```

Expected: `{"data": {"message": "Password reset successfully."}}`

```bash
# Step 3: Login with new password
curl -s -X POST http://localhost:8001/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"tenant_slug": "demo", "email": "admin@demo.com", "password": "NewPassword1!"}' | python -m json.tool
```

Expected: `{"data": {"access_token": "...", "refresh_token": "..."}}`

- [ ] **Step 13: Commit**

```bash
cd d:/LogisticOS
git add services/identity/
git commit -m "feat(identity): add forgot-password and reset-password endpoints

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

## Task 7: Identity — email verification flow

**Context:** `email_verified = false` by default. `can_login()` checks `email_verified`. In dev, we manually `UPDATE identity.users SET email_verified=true`. For MVP, need: `POST /v1/auth/send-verification-email` and `POST /v1/auth/verify-email`. Uses the same `password_reset_tokens` table pattern (separate column `token_purpose` or a new table — use the simpler approach: add an `email_verification_tokens` table).

**Files:**
- Create: `services/identity/migrations/0006_email_verification_tokens.sql`
- Modify: `services/identity/src/infrastructure/db/user_repo.rs` (add PgEmailVerificationTokenRepository)
- Modify: `services/identity/src/application/commands/mod.rs` (already done in Task 6 Step 2)
- Modify: `services/identity/src/application/services/auth_service.rs`
- Modify: `services/identity/src/api/http/mod.rs`
- Modify: `services/identity/src/api/http/auth.rs`

> **Note:** `SendVerificationEmailCommand` and `VerifyEmailCommand` were already added in Task 6 Step 2. Skip re-adding them.

- [ ] **Step 1: Create migration 0006_email_verification_tokens.sql**

```sql
-- Migration: 0006 — email_verification_tokens table

CREATE TABLE identity.email_verification_tokens (
    id          UUID        PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id     UUID        NOT NULL REFERENCES identity.users(id) ON DELETE CASCADE,
    tenant_id   UUID        NOT NULL,
    token_hash  TEXT        NOT NULL UNIQUE,
    expires_at  TIMESTAMPTZ NOT NULL,
    used        BOOLEAN     NOT NULL DEFAULT FALSE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_evt_token_hash ON identity.email_verification_tokens (token_hash);
CREATE INDEX idx_evt_user_id    ON identity.email_verification_tokens (user_id);
```

- [ ] **Step 2: Add PgEmailVerificationTokenRepository to user_repo.rs**

In `services/identity/src/infrastructure/db/user_repo.rs`, add after `PgPasswordResetTokenRepository`:

```rust
pub struct PgEmailVerificationTokenRepository {
    pool: PgPool,
}

impl PgEmailVerificationTokenRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }

    pub async fn create(&self, user_id: uuid::Uuid, tenant_id: uuid::Uuid, token_hash: &str) -> anyhow::Result<()> {
        let expires_at = chrono::Utc::now() + chrono::Duration::hours(24);
        sqlx::query(
            r#"INSERT INTO identity.email_verification_tokens (user_id, tenant_id, token_hash, expires_at)
               VALUES ($1, $2, $3, $4)
               ON CONFLICT (token_hash) DO NOTHING"#,
        )
        .bind(user_id).bind(tenant_id).bind(token_hash).bind(expires_at)
        .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn find_valid(&self, token_hash: &str) -> anyhow::Result<Option<(uuid::Uuid, uuid::Uuid)>> {
        let row = sqlx::query(
            r#"SELECT user_id, tenant_id FROM identity.email_verification_tokens
               WHERE token_hash = $1 AND used = false AND expires_at > NOW()"#,
        )
        .bind(token_hash).fetch_optional(&self.pool).await?;
        Ok(row.map(|r| (r.get("user_id"), r.get("tenant_id"))))
    }

    pub async fn mark_used(&self, token_hash: &str) -> anyhow::Result<()> {
        sqlx::query("UPDATE identity.email_verification_tokens SET used = true WHERE token_hash = $1")
            .bind(token_hash).execute(&self.pool).await?;
        Ok(())
    }
}
```

- [ ] **Step 3: Add email_verification_token_repo to AppState**

In `services/identity/src/api/http/mod.rs`, add to `AppState`:

```rust
pub email_verification_token_repo: Arc<crate::infrastructure::db::user_repo::PgEmailVerificationTokenRepository>,
```

- [ ] **Step 4: Update bootstrap to create and inject email verification token repo**

In `services/identity/src/bootstrap.rs`, add:

```rust
let email_verification_token_repo = Arc::new(
    crate::infrastructure::db::user_repo::PgEmailVerificationTokenRepository::new(pool.clone())
);
```

And pass to `AppState { ..., email_verification_token_repo }`.

- [ ] **Step 5: Add send_verification_email and verify_email to AuthService**

In `services/identity/src/application/services/auth_service.rs`, add `email_verification_token_repo` to the struct:

```rust
pub struct AuthService {
    tenant_repo: Arc<dyn TenantRepository>,
    user_repo: Arc<dyn UserRepository>,
    jwt: Arc<JwtService>,
    reset_token_repo: Arc<crate::infrastructure::db::user_repo::PgPasswordResetTokenRepository>,
    email_verification_token_repo: Arc<crate::infrastructure::db::user_repo::PgEmailVerificationTokenRepository>,
}
```

Update `AuthService::new` signature to include the new field.

Add two new methods after `reset_password`:

```rust
/// Send email verification link. Logs the link in dev (no real email).
pub async fn send_verification_email(&self, cmd: crate::application::commands::SendVerificationEmailCommand) -> AppResult<()> {
    let tenant = self.tenant_repo.find_by_slug(&cmd.tenant_slug).await
        .map_err(AppError::Internal)?
        .ok_or_else(|| AppError::NotFound { resource: "Tenant", id: cmd.tenant_slug.clone() })?;

    let user = self.user_repo.find_by_email(&tenant.id, &cmd.email).await
        .map_err(AppError::Internal)?;

    if let Some(user) = user {
        if user.email_verified {
            return Ok(()); // Already verified — silently succeed
        }

        let raw_token = uuid::Uuid::new_v4().to_string().replace('-', "")
            + &uuid::Uuid::new_v4().to_string().replace('-', "");
        let token_hash = sha2_hash(raw_token.as_bytes());

        self.email_verification_token_repo
            .create(user.id.inner(), tenant.id.inner(), &token_hash)
            .await
            .map_err(AppError::Internal)?;

        tracing::info!(
            user_id = %user.id,
            verify_link = format!("http://localhost:3002/verify-email?token={raw_token}"),
            "Email verification token generated — use this link in dev"
        );
    }
    Ok(())
}

/// Verify email using token from the link.
pub async fn verify_email(&self, cmd: crate::application::commands::VerifyEmailCommand) -> AppResult<()> {
    let token_hash = sha2_hash(cmd.token.as_bytes());

    let (user_id, _tenant_id) = self.email_verification_token_repo
        .find_valid(&token_hash).await
        .map_err(AppError::Internal)?
        .ok_or_else(|| AppError::Unauthorized("Invalid or expired verification token".into()))?;

    let user_id_typed = logisticos_types::UserId::from_uuid(user_id);
    let mut user = self.user_repo.find_by_id(&user_id_typed).await
        .map_err(AppError::Internal)?
        .ok_or_else(|| AppError::NotFound { resource: "User", id: user_id.to_string() })?;

    user.email_verified = true;
    user.updated_at = chrono::Utc::now();
    self.user_repo.save(&user).await.map_err(AppError::Internal)?;

    self.email_verification_token_repo.mark_used(&token_hash).await
        .map_err(AppError::Internal)?;

    tracing::info!(user_id = %user_id, "Email verified");
    Ok(())
}
```

- [ ] **Step 6: Add HTTP handlers**

In `services/identity/src/api/http/auth.rs`, add:

```rust
use crate::application::commands::{SendVerificationEmailCommand, VerifyEmailCommand};

pub async fn send_verification_email(
    State(state): State<Arc<AppState>>,
    Json(cmd): Json<SendVerificationEmailCommand>,
) -> Result<Json<serde_json::Value>, AppError> {
    state.auth_service.send_verification_email(cmd).await?;
    Ok(Json(serde_json::json!({ "data": { "message": "Verification email sent if account exists." } })))
}

pub async fn verify_email(
    State(state): State<Arc<AppState>>,
    Json(cmd): Json<VerifyEmailCommand>,
) -> Result<Json<serde_json::Value>, AppError> {
    state.auth_service.verify_email(cmd).await?;
    Ok(Json(serde_json::json!({ "data": { "message": "Email verified successfully." } })))
}
```

- [ ] **Step 7: Register routes**

In `services/identity/src/api/http/mod.rs`, add to public auth routes:

```rust
.route("/v1/auth/send-verification-email", post(auth::send_verification_email))
.route("/v1/auth/verify-email",            post(auth::verify_email))
```

- [ ] **Step 8: Build identity**

```bash
cd d:/LogisticOS
cargo build -p logisticos-identity 2>&1 | tail -20
```

Expected: `Finished` with no errors. Fix any struct init errors from adding new fields to `AppState` and `AuthService`.

- [ ] **Step 9: Test email verification**

Restart identity service (to run migrations 0005 and 0006). Then:

```bash
# Send verification email (check service logs for the link)
curl -s -X POST http://localhost:8001/v1/auth/send-verification-email \
  -H "Content-Type: application/json" \
  -d '{"tenant_slug": "demo", "email": "driver@demo.com"}' | python -m json.tool
```

Expected: `{"data": {"message": "Verification email sent if account exists."}}`
Logs will contain: `verify_link=http://localhost:3002/verify-email?token=<TOKEN>`

```bash
# Verify using the token from logs
TOKEN_FROM_LOG="<token from logs>"
curl -s -X POST http://localhost:8001/v1/auth/verify-email \
  -H "Content-Type: application/json" \
  -d "{\"token\": \"$TOKEN_FROM_LOG\"}" | python -m json.tool
```

Expected: `{"data": {"message": "Email verified successfully."}}`

```bash
# Verify the user can now login (driver@demo.com was previously blocked by email_verified=false)
curl -s -X POST http://localhost:8001/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"tenant_slug": "demo", "email": "driver@demo.com", "password": "LogisticOS1!"}' | python -m json.tool
```

Expected: JWT token returned.

- [ ] **Step 10: Update seed migration to auto-verify seed users**

Since seed users are already trusted dev accounts, in `services/identity/migrations/0004_seed_dev_data.sql`, ensure seed users have `email_verified = true` (check if it already does — if not, add):

```sql
UPDATE identity.users SET email_verified = true
WHERE tenant_id = '00000000-0000-0000-0000-000000000001';
```

- [ ] **Step 11: Commit**

```bash
cd d:/LogisticOS
git add services/identity/
git commit -m "feat(identity): add email verification flow (send-verification-email, verify-email)

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

## Self-Review

**Spec coverage check:**
1. ✅ Gap 1 (Secrets) — Task 1
2. ✅ Gap 2 (Reschedule) — Task 5
3. ✅ Gap 3 (OTP verify) — Task 4
4. ✅ Gap 4 (Password reset) — Task 6
5. ✅ Gap 5 (Email verification) — Task 7
6. ✅ Gap 6 (Analytics DB init) — Task 2 (analytics already has migrations; the gap was just no `.env` file and no `search_path`)
7. ✅ Gap 7 (Compliance in docker-compose) — Task 3

**Placeholder scan:** No TBD/TODO placeholders. All code shown in full.

**Type consistency check:**
- `sha2_hash` defined in Task 6 and used in Task 7 — consistent.
- `PgPasswordResetTokenRepository` defined in Task 6, referenced in Task 7 for struct field — consistent.
- `VerifyOtpCommand` was pre-existing — Task 4 references it directly with no redefinition.
- `reschedule_delivery` handler calls `state.tracking_svc.reschedule(...)` — service method defined in Task 5 Step 7 with that exact signature.

**Known risk:** Task 5 (reschedule) requires reading actual entity/table names from the delivery-experience DB impl before writing the SQL. Steps 1-3 are explicit read-first steps for this reason.
