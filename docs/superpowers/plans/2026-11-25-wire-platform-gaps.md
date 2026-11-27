# Wire the Platform Gaps — Implementation Plan (2026-11-25)

**Spec:** none separate. This plan is the authority. It was built from code reconnaissance on `origin/master` @ `cbd0c636` and four decisions the architect made on 2026-11-25:
1. Kafka orphaned partitions are repaired on the VPS by the controller (not part of any task here).
2. `/mcp` OAuth: per-tool RBAC ships now; the authorization server becomes an ADR and plan only (controller writes it).
3. Vehicle marketplace: wire the merchant UI to the real carrier backend now. ADR-0014 rental and On-Demand get an implementation plan only (controller writes it).
4. Delivery PIN: **required only if it was actually delivered** to the recipient. If it never reached them, POD falls back to photo plus the existing geofence, and the POD is flagged `PIN_UNDELIVERED`.

## Global Constraints (bind every task)

- **Two worktrees, no others.**
  - Rust lane: `D:\LogisticOS\.worktrees\wire-gaps`.
  - Web lane: `D:\LogisticOS\.worktrees\wire-gaps-web`.
  - Never touch `D:\LogisticOS` (the main checkout) or any other worktree.
  - Each task names its branch. If the branch does not exist yet, create it from `origin/master`: `git checkout -b <branch> origin/master`. If it exists, check it out and continue on it.
- **Git.** Do not push. Do not merge. Never use bare `git stash`.
  - Commit messages are conventional (`type(scope): summary`). Write the message to a temp file and use `git commit -F <file>`.
  - End every commit message with the line `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
  - Never commit `.superpowers/`, `node_modules/`, `.env*` or `settings.local.json`.
- **Rust verification** (every Rust task):
  - Run every cargo command with `CARGO_INCREMENTAL=0`. The target dir is already configured in `.cargo/config.toml`; do not override it.
  - The gate is all three of:
    - `cargo check -p <crate>`
    - `cargo clippy -p <crate> --all-targets -- -D warnings`
    - `cargo test -p <crate>` (for each crate touched)
  - Tests that need a live Postgres or Kafka and cannot run locally are reported as "not run", never claimed.
  - `link.exe` exit 1318/1180/1140 and "failed to mmap rmeta" are disk-full, not code errors. If you hit one, report BLOCKED with the message; do not delete anything.
- **Rust code rules:**
  - No `unwrap()` or `expect()` in production paths; use `?` with `anyhow`/`thiserror` and the crate's `AppError`.
  - Validate at the API boundary.
  - Every mutation that has an audit pattern in that service follows it.
  - Observability routes (`/health`, `/ready`, `/metrics`) stay outside auth layers.
- **Migrations:**
  - Use the next free number in that service's `migrations/`.
  - Additive only, `IF NOT EXISTS`, safe for the running image to keep working before the new image rolls out (ADR-0012 per-schema `_sqlx_migrations`).
  - A migration that can fail on existing data must check first and abort with a clear message, as `services/driver-ops/migrations/0014_collapse_driver_id_onto_user_id.sql` does.
- **Kafka:**
  - A new topic needs three things:
    - a constant in `libs/events/src/topics.rs`
    - a `create_topic` line in `scripts/create-kafka-topics.sh`
    - a line in `scripts/kafka/create-topics.sh`, using the same partitions/RF/retention as neighbouring topics
  - Then run `bash scripts/check-kafka-topics.sh` and it must pass.
  - Publishers use `KafkaProducer::publish_event` with `libs/events` `Event::new(source, event_type, tenant_id, data)`.
  - A publish failure after a committed DB change is logged at `error` and never fails the user's request, unless the task says otherwise.
- **Frontend verification** (every portal task):
  - In the app directory: delete `tsconfig.tsbuildinfo` if present, then run `npx tsc --noEmit`, then `npx next lint`. ESLint **errors** fail CI; warnings do not. Grep the output for `Error:`.
  - Run `npm run build` when you touch a page that uses `useSearchParams`, or when you add a page.
  - If `node_modules` is missing in that app, run `npm ci` there first.
  - Use the app's existing `authFetch` and API base env var. Never hardcode a host.
  - All user-visible strings follow the app's existing i18n pattern if the file you edit uses one.
- **Permissions** are the constants in `libs/auth/src/rbac.rs` (`permissions::*`). Never use string literals in Rust for a permission that has a constant.
- **No subagents.** Implementers never dispatch subagents or reviewers.
- **Report contract.**
  - Write the full report to the report path given in the dispatch: what changed, files, commits, the verification commands run with their pass/fail, anything not run, and concerns.
  - Return only: status (DONE / DONE_WITH_CONCERNS / NEEDS_CONTEXT / BLOCKED), commit SHAs, a one-line test summary, and concerns.

---

## Rust lane

### Task R1: `/mcp` per-tool RBAC, bearer forwarding, broken tool routes

**Branch:** `fix/ai-layer/mcp-per-tool-rbac`
**Files:**
- `services/ai-layer/src/api/mcp/mod.rs`
- `services/ai-layer/src/infrastructure/tools/mod.rs`
- possibly `libs/agent-runtime/src/tools.rs` (read-only unless needed)

**Problem.**
- Every `enterprise_mcp` caller can list and call every tool.
- 19 of 21 tools forward no `Authorization` header. So tools behind JWT-protected routes 401, and the three tools that call unauthenticated `/v1/internal/*` routes run with no permission check at all: `assign_driver`, `get_available_drivers` and `get_cod_balance`.
- Several tools call routes that do not exist.

**Requirements:**
1. **One MCP permission table** in the mcp module: `tool name → (any_of: &'static [&'static str], internal_route: bool)`. Use the `permissions::*` constants. Exact table:

   | tool | any_of | internal_route |
   |---|---|---|
   | get_shipment | SHIPMENT_READ (`shipments:read`) | false |
   | reschedule_delivery | SHIPMENT_UPDATE (`shipments:update`) | false |
   | get_available_drivers | `dispatch:view` | **true** |
   | assign_driver | `dispatch:assign` | **true** |
   | send_notification | `engagement:send` | false |
   | get_delivery_metrics | `analytics:view` | false |
   | get_driver_performance | `analytics:view` | false |
   | reconcile_cod | `payments:reconcile` | false |
   | generate_invoice | the constant `services/payments/src/api/http/invoices.rs:138` requires (`BILLING_MANAGE`, `payments:reconcile`); verify and use that constant | false |
   | get_cod_balance | `payments:read` | **true** |
   | get_customer_profile | `customers:read` | false |
   | get_churn_score | `customers:read` | false |
   | get_customer_preferences | `customers:read` | false |
   | get_hub_capacity | `fleet:read` | false |
   | get_driver_location | `fleet:read` | false |
   | send_driver_instruction | `fleet:manage` | false |
   | get_vehicle_status | `fleet:read` | false |
   | get_fleet_availability | `fleet:read` | false |

   **Not exposed over MCP** (an explicit `MCP_HIDDEN` list):
   - `get_zone_demand_forecast` (no downstream route exists)
   - `schedule_dock` (no downstream route exists)
   - `escalate_to_human` (support-agent loop marker only)
2. **Fail closed.** A tool with no table entry is not listed and not callable over MCP.
3. **A unit test fails** if any tool registered in the `ToolRegistry` is in neither the table nor `MCP_HIDDEN`. That makes adding a tool force a decision.
4. **Access rule.** One function `mcp_tool_access(claims, tool) -> Result<(), Denied>`, used by **both** `list_tools` and `call_tool`, so they cannot disagree:
   - allowed iff the table has the tool;
   - AND `claims.has_permission(p)` for at least one `p` in `any_of` (this honours `*`);
   - AND, if `internal_route`, `claims.pid.is_none()`. Partner-scoped callers never reach unscoped internal routes.
5. **Listing.** `list_tools` returns only the definitions the caller may access.
6. **Calling.** `call_tool` checks before executing.
   - A refusal returns an MCP error whose message names the tool and the permissions required.
   - The refusal is also written through the existing `record_audit` path as a failed action, with `success = false` or the existing failure representation. Do not add a new table.
7. **Bearer forwarding.** Every tool whose downstream route is JWT-protected must forward the caller's bearer when `ToolContext` carries one, using the existing `register_ctx` / `authed` helpers:
   - send_notification
   - get_delivery_metrics, get_driver_performance
   - reconcile_cod, generate_invoice
   - get_customer_profile, get_churn_score, get_customer_preferences
   - get_hub_capacity
   - get_driver_location, send_driver_instruction
   - get_vehicle_status, get_fleet_availability

   With no bearer (autonomous agents), the request is byte-for-byte what it is today.
8. **Route fixes** (read the downstream handler to get the exact request/response shape):
   - `send_notification` → `POST {engagement}/v1/notifications/send`. Map the tool's input to that route's body. If a required field of that body cannot be derived from the tool's declared input schema, extend the tool's input schema.
   - `get_vehicle_status` → `GET {fleet}/v1/fleet/vehicles/:id`.
   - `get_fleet_availability` → `GET {fleet}/v1/fleet/vehicles`.
9. **Keep** the server-side `tenant_id` overwrite in `call_tool` exactly as is.
10. **Tests (unit, no network):**
    - the registry/table coverage test;
    - a `merchant` role's `default_permissions_for_role` cannot access `assign_driver`, `generate_invoice` or `reconcile_cod`;
    - an `admin` with `pid = Some(..)` cannot access the three internal-route tools but can access `get_shipment`;
    - `*` can access every table tool and none of `MCP_HIDDEN`;
    - `list_tools` filtering and the `call_tool` check give the same answer for every (role, tool) pair over the role list in `rbac.rs`.

### Task R2: Compliance — admin uploads a document on a courier's behalf

**Branch:** `feat/compliance/admin-upload-on-behalf`
**Files:**
- `services/compliance/src/api/http/{mod.rs,admin_routes.rs,driver_routes.rs}`
- `services/compliance/src/application/services/compliance_service.rs`

**Problem.** A field worker with no app cannot leave `pending_submission`. The admin routes are review-only.

**Requirements:**
1. **New route** `POST /api/v1/compliance/admin/profiles/:profile_id/documents`.
   - Permission `permissions::COMPLIANCE_ADMIN` (`compliance:admin`).
   - Layer `DefaultBodyLimit::max(MAX_UPLOAD_BODY_BYTES)` on **this route only**, exactly as the self-upload route does.
2. **Body:** the same JSON as the courier self-upload `UploadDocumentRequest`. Reuse the type; do not copy it.
   - `document_type_id?` | `document_type_code?`, `document_number`, `file_base64`, `content_type`, `issue_date?`, `expiry_date?`
   - Same validation: one of the type fields required; trimmed non-empty `document_number` ≤ 100 chars; valid base64; dates `YYYY-MM-DD`.
3. **Shared helpers.** Move `resolve_document_type_id` and `parse_date` (and `UploadDocumentRequest` if needed) out of `driver_routes.rs` into a shared module both route files use. No duplicated logic.
4. **Tenant check.** Load the profile by id. If missing, or `profile.tenant_id != claims.tenant_id`, return 404 using the same pattern as `admin_routes.rs` profile reads.
5. **Storage.** Upload through `state.storage.upload(tenant_id, bytes, &content_type)`, with the same type and size limits.
6. **Record the real actor.** `ComplianceService::submit_document` takes an actor instead of a bare `actor_id`: `enum DocumentActor { Driver(Uuid), Admin(Uuid) }`.
   - The audit row's `actor_type` is `"driver"` or `"admin"` accordingly, and `actor_id` is the respective user id.
   - The courier path passes `Driver(claims.user_id)` and behaves exactly as today.
   - The document is still inserted as `Submitted`. Nothing is auto-approved; a reviewer still approves.
   - `recompute_and_publish` runs as today.
7. **Response:** `201 Created` with `{ "data": <DriverDocument> }`.
8. **Tests:**
   - the audit `actor_type` for both actors;
   - a caller with only `compliance:review` is refused (403);
   - a profile from another tenant is 404;
   - an invalid body is a validation error.

   Prefer the crate's existing test style (unit or `tests/` harness). Tests that need a DB follow the existing harness or are reported "not run".

### Task R3: business-logic — read the real event envelope; threshold triggers

**Branch:** `feat/business-logic/threshold-triggers` (shared by R3–R7, in order)
**Files:**
- `services/business-logic/src/{bootstrap.rs,application/services/mod.rs}`
- `libs/events/src/{topics.rs,payloads.rs}`
- the topic scripts

**Problem.** Every `libs/events` publisher sends `Event { id, source, event_type, time, tenant_id, data }` and sets **no Kafka headers**. business-logic reads the tenant **only** from a `tenant_id` header, and reads ids from the payload's top level. So:
- the tenant is always unknown, and no tenant-scoped rule ever matches;
- `shipment_id`, `driver_id` and the rest are always `None`.

Separately, the `HubCapacityExceeded`, `PaymentOverdue` and `SlaBreach` triggers have no topic mapping.

**Requirements:**
1. **Unwrap the envelope.** Add `fn decode_event(headers_tenant: Option<Uuid>, payload: &Value) -> Option<(Uuid, Value)>`.
   - **Envelope form:** `payload` is an object with an object `data` and a uuid-string `tenant_id`. Tenant = envelope `tenant_id`; data = `payload["data"]`.
   - **Flat form:** tenant = header, else the payload's own `tenant_id`; data = payload.
   - **No tenant determinable:** log `warn` with the topic, skip the message and commit its offset. Rules are not evaluated with `Uuid::nil()` any more.
   - `build_context(tenant, topic, data)` reads from `data`, and `metadata` is `data`.
2. **Audit every publisher.** For each of the 9 currently subscribed topics, find its publisher(s) (`grep -rn "<CONSTANT>"` over `services/*/src`) and record in the report a table: topic | publisher file:line | envelope or flat | fields business-logic reads that are actually present. Do not change publishers in this task; report any topic with no publisher.
3. **New topic constants** in `libs/events/src/topics.rs`:
   - `HUB_LOAD_CHANGED = "logisticos.hub.load.changed"`
   - `INVOICE_OVERDUE = "logisticos.payments.invoice.overdue"`
   - `SHIPMENT_SLA_BREACHED = "logisticos.order.shipment.sla_breached"`

   Add them to both topic scripts and pass `scripts/check-kafka-topics.sh`.
4. **Payload structs** in `libs/events/src/payloads.rs` (Serialize + Deserialize, snake_case), used by R4, R5 and R7:
   - `HubLoadChanged { hub_id: Uuid, capacity: i32, previous_load: i32, current_load: i32 }`
   - `InvoiceOverdue { invoice_id: Uuid, merchant_id: Uuid, amount_cents: i64, currency: String, due_at: DateTime<Utc>, days_overdue: i32, previous_days_overdue: i32 }`. Use the invoice's actual merchant/amount field types if they differ; record any deviation in the report.
   - `ShipmentSlaBreached { shipment_id: Uuid, merchant_id: Uuid, service_type: String, sla_type: String, sla_due_at: DateTime<Utc>, breached_at: DateTime<Utc> }`
5. **Replace the matcher.** `trigger_matches_topic(trigger, topic)` becomes `trigger_matches(trigger, topic, data) -> bool`. Keep every existing arm unchanged and add:
   - `HubCapacityExceeded { threshold_pct }` on `HUB_LOAD_CHANGED`: true iff `capacity > 0 && previous_load*100 < threshold_pct*capacity && current_load*100 >= threshold_pct*capacity`. This is an **upward crossing**, in integer math, widened to i64.
   - `PaymentOverdue { days }` on `INVOICE_OVERDUE`: true iff `previous_days_overdue < days as i32 && days_overdue >= days as i32`.
   - `SlaBreach { sla_type }` on `SHIPMENT_SLA_BREACHED`: true iff `data.sla_type` equals `sla_type`, ASCII case-insensitive.
   - A missing or ill-typed field means false.
   - `rules_for_topic` takes the data and uses `trigger_matches`.
6. **Subscribe** to the three new topics.
7. **Tests (unit):**
   - `decode_event` for the envelope form, flat-with-header, flat-with-body-tenant and no-tenant;
   - each new arm: below → no; crossing → yes; already above → no; missing field → no;
   - an existing arm (`DeliveryFailed`) still matches with an envelope payload, and its context gets `shipment_id` from `data`.

### Task R4: payments — invoices actually become overdue, and say so

**Branch:** `feat/business-logic/threshold-triggers`
**Files:**
- `services/payments/src/{bootstrap.rs,application/services/invoice_service.rs or a new sweep module,infrastructure/db/...}`
- a migration

**Problem.**
- `Invoice::check_overdue` exists, but nothing calls it.
- No code ever moves an invoice to `overdue`.
- No event is published.

**Requirements:**
1. **Migration:** `ALTER TABLE payments.invoices ADD COLUMN IF NOT EXISTS overdue_days_notified INTEGER NOT NULL DEFAULT -1;`
2. **Sweep.** Add an overdue sweep run from bootstrap next to the existing subscription sweep (`bootstrap.rs` ~230–250, same structure).
   - Interval: config `PAYMENTS__OVERDUE_SWEEP_SECS`, default `3600`. Follow how the crate reads config with defaults.
   - Each tick, two steps, each a bounded batch (500):
     - **Transition:** move invoices with `status = 'issued' AND due_at < now()` to `overdue` (bump `updated_at`). The rule must agree with `Invoice::check_overdue`; reuse it or keep a test proving the SQL and the domain method agree.
     - **Notify.** For `status = 'overdue'` invoices:
       1. Compute `d = floor((now - due_at) / 1 day)` as i32.
       2. Where `d > overdue_days_notified`, claim atomically with `UPDATE ... SET overdue_days_notified = $d WHERE id = $id AND overdue_days_notified = $prev RETURNING ...`.
       3. Publish `INVOICE_OVERDUE` with `InvoiceOverdue { days_overdue: d, previous_days_overdue: prev }`. The envelope tenant is the invoice tenant.
       4. **If the publish fails, put the claim back** (`SET overdue_days_notified = $prev WHERE id = $id AND overdue_days_notified = $d`) so the next tick retries.
3. **Payment still clears an overdue invoice.** Paying an invoice in `overdue` must still move it to `paid`. Check the domain transition and every payment-received path. If `Overdue → Paid` is refused anywhere, fix it and test it.
4. **Tests:**
   - the day computation, including the boundary (exactly 24h → 1, 23h59m → 0);
   - the claim/rollback decision logic as a pure function where possible;
   - `Overdue → Paid`.

### Task R5: hub-ops — publish load changes

**Branch:** `feat/business-logic/threshold-triggers`
**Files:** `services/hub-ops/src/...` (`HubService` paths that change `current_load`).

**Requirements:**
1. Find every code path that changes `hub_ops.hubs.current_load`: `Hub::induct_parcel`/`dispatch_parcel` through `HubService`, plus any direct SQL in `bootstrap.rs` or repos (`grep -rn current_load services/hub-ops/src`).
2. After each successful save that changed it, publish `HUB_LOAD_CHANGED` with `HubLoadChanged { hub_id, capacity, previous_load, current_load }`.
   - Capture `previous_load` **before** the mutation.
   - A publish failure is logged at `error` and never fails the request.
   - Use the producer the service already holds.
3. Report every `current_load` writer you found and whether each now publishes. If a writer is raw SQL with no previous value at hand, use `UPDATE ... RETURNING` to obtain old and new values. Do not do a second read.
4. **Tests:** the event built for an induction and a dispatch carries the right previous/current values (unit, around the event construction).

### Task R6: driver idle — driver-ops reports, business-logic polls

**Branch:** `feat/business-logic/threshold-triggers`
**Files:**
- `services/driver-ops/src/{api/http/...,infrastructure/db/...}`
- `services/business-logic/src/bootstrap.rs` (poller)
- business-logic config

**Design:** idle is the absence of events, so it is detected by a poll with the rule's own threshold. The precedent is the `CustomerInactive` hourly poller in business-logic.

**Requirements:**
1. **driver-ops internal route** `GET /v1/internal/drivers/idle?tenant_id=<uuid>&idle_minutes=<u32>&window_secs=<u32>`.
   - Mount and protect it exactly as driver-ops' other `/v1/internal/*` routes are. If driver-ops has none, follow dispatch's internal-route pattern and say which in the report.
   - Response: `{ "data": [ { "driver_id": <uuid>, "idle_since": <rfc3339>, "idle_minutes": <i64> } ] }`. `driver_id` is the driver's `user_id`.
   - A driver is idle when **all** of these hold:
     - in `tenant_id`;
     - `status = 'available'`;
     - has an open duty session (`duty_sessions.ended_at IS NULL`);
     - has no task in an active (non-terminal) status. Derive the set from the task status enum and list it in the report.
   - `idle_since = GREATEST(open duty session started_at, MAX(tasks.completed_at) for that driver)`.
   - Return only drivers with `idle_since <= now() - idle_minutes` **and** `idle_since > now() - idle_minutes - window_secs`. This is the **crossing window**: each idle episode is returned in exactly one poll.
   - Validate `idle_minutes` in 1..=1440 and `window_secs` in 30..=3600, else 400.
   - Bounded: `LIMIT 1000`.
2. **business-logic poller** `run_idle_driver_checker`, added to the same `tokio::select!` as the inactive checker.
   - Interval: config `BUSINESS_LOGIC__IDLE_POLL_SECS`, default `60`, passed as `window_secs`.
   - Driver-ops base URL from config, following how `cdp_url` is configured. Name it consistently with the crate's existing service URLs.
   - Each tick:
     1. Load active `DriverIdleExceeded { minutes }` rules, grouped by (tenant, minutes).
     2. Skip rules with `tenant_id == Uuid::nil()`. Log once at `info` per process that global idle rules are not supported.
     3. Call driver-ops.
     4. For each driver, build the context with pseudo-topic `"logisticos.driver.idle_exceeded"` and data `{ "driver_id", "tenant_id", "idle_since", "idle_minutes" }`, then run `enrich_segments`, `conditions_met` and `execute_actions`.
     5. **Log the execution** with `pg_repo.log_execution`, as the Kafka path does. Unlike the CustomerInactive path, the new poller must log.
   - A driver-ops error is logged and the tick continues with the next group.
3. **Tests:**
   - the window boundaries, as a pure function or SQL-parameter computation;
   - query validation (400s);
   - rule grouping, including the nil-tenant skip.

### Task R7: order-intake — a delivery promise, and its breach

**Branch:** `feat/business-logic/threshold-triggers`
**Files:**
- `services/order-intake/src/application/services/shipment_service.rs` (`sla_label`)
- the shipment create path, repo and bootstrap
- a migration

**Design:**
- The promise the customer is already shown (`sla_label`) becomes a stored deadline. A sweep marks breaches and publishes.
- Label and deadline come from **one table**, so they cannot drift.
- **No backfill.** Existing shipments get no deadline and never breach. This avoids an alert storm for historical open shipments.

**Requirements:**
1. **Migration** (use the schema the shipments table actually lives in):
   ```sql
   ALTER TABLE <schema>.shipments ADD COLUMN IF NOT EXISTS sla_due_at TIMESTAMPTZ;
   ALTER TABLE <schema>.shipments ADD COLUMN IF NOT EXISTS sla_breached_at TIMESTAMPTZ;
   CREATE INDEX IF NOT EXISTS idx_shipments_sla_open
       ON <schema>.shipments (sla_due_at)
       WHERE sla_breached_at IS NULL AND sla_due_at IS NOT NULL;
   ```
2. **Promise table.** Replace `sla_label`'s match with a `ServicePromise` table that gives both the label (unchanged strings) and a deadline rule:

   | service_type | label (unchanged) | deadline |
   |---|---|---|
   | `same_day` | "Today" | end of the base day |
   | `express` | "Next business day" | end of the 1st business day after the base day |
   | `international` | "7–14 business days" | end of the 14th business day after |
   | `balikbayan` | "21–45 business days" | end of the 45th business day after |
   | anything else (standard) | "2–3 business days" | end of the 3rd business day after |

   - "End of day" is 23:59:59 in the fixed offset `SLA__UTC_OFFSET_HOURS` (config, default `8`).
   - Business days are Mon–Fri. There is no holiday calendar; document it in a code comment.
   - **Base:** `max(created_at, scheduled_pickup_at)`.
   - Keep `sla_label`'s callers working, returning identical strings.
3. **Set `sla_due_at`** in the single shared shipment create path. Find it; there must be one, and the memory note says "don't add a second create path". Every new shipment gets one.
4. **Sweep** in bootstrap.
   - Interval: config `SLA__SWEEP_SECS`, default `300`.
   - Each tick:
     1. Claim breaches with `UPDATE ... SET sla_breached_at = now() WHERE id IN (SELECT id FROM ... WHERE sla_due_at < now() AND sla_breached_at IS NULL AND status NOT IN (<terminal>) ORDER BY sla_due_at LIMIT 500 FOR UPDATE SKIP LOCKED) RETURNING ...`.
     2. For each claimed row, publish `SHIPMENT_SLA_BREACHED` with `sla_type: "delivery"`.
     3. **If the publish fails, reset `sla_breached_at` to NULL** for that id so the next tick retries.
   - `<terminal>` is derived from the shipment status enum (delivered, cancelled, returned, and any other final state). List it in the report.
5. **Tests:** the promise table, as pure functions with fixed timestamps:
   - Friday 18:00 +08 `express` → Monday 23:59:59 +08;
   - Saturday `standard` → Wednesday 23:59:59 +08;
   - `same_day` → same day 23:59:59 +08;
   - a `scheduled_pickup_at` later than `created_at` moves the base;
   - labels unchanged.

### Task R8: pod — the PIN is required only if it reached the recipient

**Branch:** `feat/pod/pin-required-only-if-delivered`
**Files:**
- `services/pod/src/{domain/value_objects/delivery_pin.rs,domain/entities/otp.rs,application/services/pod_service.rs,api/http/pod.rs,infrastructure/external/sms.rs,infrastructure/db/...}`
- a migration

**Problem.**
- pod runs `NoOpSmsAdapter` when Twilio is unset, so shipments booked outside the customer app never get their PIN.
- With `DELIVERY_PIN__REQUIRED=true`, every such delivery is refused at the door.
- `NoOpSmsAdapter` also **logs the message body, which contains the plaintext PIN**.

**Requirements:**
1. **Migration (next number):**
   ```sql
   ALTER TABLE pod.otp_codes ADD COLUMN IF NOT EXISTS delivered_via TEXT;
   ALTER TABLE pod.otp_codes ADD COLUMN IF NOT EXISTS delivered_at TIMESTAMPTZ;
   ALTER TABLE pod.proofs ADD COLUMN IF NOT EXISTS pin_exception TEXT;
   ```
   Map the columns in the repo and entities (`OtpCode.delivered_via: Option<String>`, `delivered_at: Option<DateTime<Utc>>`; the POD gets `pin_exception: Option<String>`).
2. **`NoOpSmsAdapter::send`** returns `Err(anyhow!("no SMS provider configured"))`. It logs at `warn` **without the body** (recipient only, masked to the last 4 digits).
3. **Recording delivery.**
   - `GenerateOtpCommand` gains `reveal_to_caller: bool`. The handler sets it from `code_is_visible_to(&claims.roles)`.
   - In `generate_and_send_otp`, for a **new** PIN:
     - `delivered_via = "sms"` if `sms.send` returned `Ok`;
     - else `"response"` if `reveal_to_caller`;
     - else NULL.
   - `delivered_at = now()` when `delivered_via` is set.
   - Persist with the PIN; do not do a second write if avoidable.
   - `KeptLive` changes nothing.
   - The response shape is unchanged.
4. **Pure decision function** in `delivery_pin.rs`:
   `pin_requirement(policy_required: bool, supplied: bool, live_pin: Option<LivePinState>) -> PinRequirement`, where `LivePinState { delivered: bool }` and `PinRequirement` is `Check` | `RefuseMissing` | `Waive` | `NotRequired`.

   | case | result |
   |---|---|
   | `supplied` | `Check` (a supplied PIN is always checked) |
   | `!supplied && !policy_required` | `NotRequired` |
   | `!supplied && policy_required && live_pin.delivered` | `RefuseMissing` |
   | `!supplied && policy_required && (no live PIN \|\| !delivered)` | `Waive` |
5. **Submit path.** Use `pin_requirement`. On `Waive`:
   - the POD must have **at least one photo** of evidence, else `BusinessRule("A photo of the delivered parcel is required when the recipient never received a delivery PIN.")`;
   - set `pod.pin_exception = Some("PIN_UNDELIVERED")`;
   - append a telemetry log entry `event_type = "POD_PIN_UNDELIVERED"` through the telemetry repository pod already holds;
   - log at `warn`.

   The existing geofence and evidence checks are unchanged. The PIN-burn ordering is unchanged.
6. **Android check, report only.** Read `apps/driver-app-android` POD capture flow and report whether the client lets a driver submit without entering a PIN. Do not change Android.
7. **Tests:**
   - `pin_requirement`, all rows;
   - `NoOpSmsAdapter` returns `Err`;
   - `delivered_via` selection (sms ok → `sms`; sms err + reveal → `response`; sms err + no reveal → None);
   - Waive without a photo is refused;
   - Waive with a photo sets `pin_exception`.

### Task R9: carrier — a merchant can cancel a booking the carrier has not accepted

**Branch:** `feat/carrier/marketplace-merchant-cancel`
**Files:** `services/carrier/src/{api/http/mod.rs,application/services/marketplace.rs,domain/entities/mod.rs}`

**Requirements:**
1. **New route** `POST /v1/marketplace/bookings/:booking_id/cancel`.
   - Permission `permissions::MARKETPLACE_BOOK`.
   - Load the booking. Return 404 unless `booking.tenant_id == claims.tenant_id && booking.booked_by_user_id == Some(claims.user_id)`.
2. **Only `Pending` can be cancelled.** Any other status gives `BusinessRule("Only a booking the carrier has not accepted yet can be cancelled.")`.
3. **Shared money path.** Refactor `expire_booking`'s void-then-cancel logic into one private function used by both expiry and merchant cancel. **Void first, then cancel**, so a failed void leaves the booking `Pending` and returns an error. Keep the money-safety comment.
   - Add a distinct domain transition `cancel_by_consumer()` (Pending → Cancelled).
   - The expiry path keeps `expire()`.
   - Log which one happened.
4. **Response:** `{ "booking": <MarketplaceBooking> }`. This mirrors `create_booking`'s top-level key style; check it and match.
5. **Tests:**
   - domain `cancel_by_consumer` from Pending works, and from Accepted fails;
   - the shared void path is called for `Authorized` and skipped otherwise (unit, with the existing test doubles if any).

### Task R10: driver identity — remove the split-brain's residue

**Branch:** `chore/driver-ops/driver-id-residue`

**Context.** `driver-ops` migration 0014 (`8b5fa214`) already enforces `CHECK (id = user_id)` on `driver_ops.drivers`. What remains is residue that still reads as if the two could differ, plus the same unenforced invariant in field-ops.

**Requirements:**
1. **Joins.** In `services/driver-ops/src/infrastructure/db/task_repo.rs`, `list_earnings` and `daily_earnings` still `JOIN driver_ops.drivers d ON d.id = t.driver_id WHERE d.user_id = $1`. Replace them with direct `WHERE t.driver_id = $1`, and delete their "may differ for API-registered drivers" comments.
2. **Stale comments.** Delete or correct the stale comments that claim ids can differ:
   - `task_consumer.rs` ~89–91 and ~100–102
   - `dispatch/src/infrastructure/db/driver_avail_repo.rs` ~42–44
   - `driver-ops task_service.rs` ~608–609
   - the `JobDropped` doc comment in `libs/events/src/payloads.rs` ~640

   Say instead that `drivers.id = user_id` is enforced by migration 0014. Behaviour must not change. Keep any existence check that a lookup performs.
3. **field-ops.** `field_ops.couriers` has `id DEFAULT gen_random_uuid()` and `user_id`. Code sets `id = user_id`, but nothing enforces it.
   - Verify **every** insert path into `field_ops.couriers` sets `id = user_id`. List them in the report.
   - Add a field-ops migration modelled on driver-ops 0014: abort with a clear message if any row has `id <> user_id`; drop the `id` default; add `CHECK (id = user_id)` named `couriers_id_is_user_id`; add a column comment.
4. **Gateway.** `services/api-gateway/src/proxy/mod.rs` sends every `/v1/drivers*` path to driver-ops, so dispatch's `POST /v1/drivers/:id/cancel-assignment` looks unreachable through the gateway. The admin portal calls it from `apps/admin-portal/src/lib/api/dispatch.ts:202`.
   - Verify by reading `resolve_upstream` and dispatch's router.
   - If confirmed, route exactly that path shape to dispatch, ahead of the generic rule, and add a unit test in the gateway's existing test style.
   - If not confirmed, change nothing and explain.
5. **Gates.** Run the Rust gates for driver-ops, dispatch, field-ops, libs/events and api-gateway (only those touched).

### Task R11a: wire the dead test suites — api-gateway

**Branch:** `chore/ci/wire-dead-test-suites` (shared by R11a–R11d)

**Recipe** (from the 2026-08-16 carrier/identity wiring; follow it in this order):
1. Add `[[test]] name = "<integration|unit>" path = "tests/<dir>/mod.rs"` to the crate's Cargo.toml, then run `cargo test -p <crate> --no-run` and let the compiler enumerate the drift. Do not trust stale comments that predict the errors.
2. Fix the **tests** to match today's APIs. Never change production code to fit a stale test. If a test asserts behaviour that no longer exists, delete that test and list it in the report with the reason.
3. Concrete `Pg*Repository` constructor args need no database: use `PgPoolOptions::new().connect_lazy(url)`.
4. Kafka: use `rdkafka::mocking::MockCluster` (not feature-gated in rdkafka 0.36). Assert `!bootstrap_servers().is_empty()`.
5. Mount `require_auth` exactly as bootstrap does.
6. **Never copy a helper out of a dead suite without compiling it.** For example, pod's suite calls `KafkaProducer::noop()`, which does not exist.
7. **Database tests.** Tests that genuinely need Postgres follow whatever the crate's already-wired DB tests do (env var plus skip, or `#[ignore]`). Check `.github/workflows/ci-rust.yml` for how CI provides Postgres to the matrix, and report how these run there.

**This task:** `services/api-gateway/tests/integration/mod.rs` and `services/api-gateway/tests/unit/mod.rs`.

### Task R11b: wire the dead test suites — carrier and identity integration

**Branch:** `chore/ci/wire-dead-test-suites`
- Same recipe as R11a.
- Targets: `services/carrier/tests/integration/mod.rs` and `services/identity/tests/integration/mod.rs`.
- A 2026-08-16 note claims these were wired. The scan on `origin/master` shows no `[[test]]` entry, so verify which is true and report.

### Task R11c: wire the dead test suites — order-intake and pod

**Branch:** `chore/ci/wire-dead-test-suites`
- Same recipe as R11a.
- Targets:
  - `services/order-intake/tests/integration/mod.rs`
  - `services/order-intake/tests/unit/mod.rs`
  - `services/pod/tests/integration/mod.rs`

### Task R11d: wire the dead test suites — delivery-experience, cdp, marketing

**Branch:** `chore/ci/wire-dead-test-suites`
- Same recipe as R11a.
- Targets:
  - `services/delivery-experience/tests/integration/mod.rs`
  - `services/cdp/tests/integration/mod.rs`
  - `services/marketing/tests/integration/mod.rs`
- Known difficulty:
  - marketing's `AppState` holds concrete `Pg*Repository` types; try `connect_lazy` before considering any refactor.
  - cdp's `SegmentRepository` has ten methods and needs a test double.
- Report any suite you could not wire, with the reason.

### Task R12: production `unwrap()` → zero, then make the CI step a gate

**Branch:** `chore/ci/unwrap-gate`

**Requirements:**
1. **Enumerate** with the exact CI command: `cargo clippy --workspace --lib --bins --all-features -- -W clippy::unwrap_used`. If the workspace build is too heavy for disk, iterate per crate with the same flags (`-p <crate> --lib --bins`). Record the starting count per crate in the report.
2. **Replace each production `unwrap()`** with proper error propagation (`?` with the crate's error type, `ok_or_else`, `map_err`).
   - Where the value is provably present (a compile-time-constant regex, header value or selector), use `expect("<invariant>")`. **Only** with a precise invariant message.
   - Count these separately in the report. The CI flag is `unwrap_used`, so `expect` is allowed, but keep `expect` rare and justified.
   - Do not change behaviour beyond turning a panic into an error return on the impossible path.
3. **Flip the CI step** in `.github/workflows/ci-rust.yml`:
   - drop `continue-on-error: true`;
   - run `cargo clippy --workspace --lib --bins --all-features -- -D clippy::unwrap_used`;
   - remove the `|| true` grep pipeline;
   - rename the step to drop "(advisory)";
   - update the comment block to state the gate and the date.
4. **Gates:** the workspace clippy command above with `-D clippy::unwrap_used` passes, plus `cargo test -p <crate>` for every crate touched.

---

## Web lane (`D:\LogisticOS\.worktrees\wire-gaps-web`)

### Task W1: admin-portal venues page — `parseLabels` breaks the type check

**Branch:** `fix/admin-portal/venues-parse-labels`

**Problem.** `apps/admin-portal/src/app/(dashboard)/venues/[venueId]/page.tsx:52` exports `parseLabels` from a Next.js page file. Page files may only export Next's allowed names, so the type check breaks. Nothing imports it.

**Requirements:**
1. Move `parseLabels` unchanged to `apps/admin-portal/src/lib/venues/parse-labels.ts` (named export) and import it back into the page.
2. Add a unit test next to the module in the portal's existing test framework, covering the current behaviour. Read the function and assert its real outputs: commas/whitespace, empty input, duplicates — whatever it actually does.
3. Run the frontend gates for admin-portal. The `tsc` error must be gone.

### Task W2: admin-portal — "Upload document" on a courier's compliance profile

**Branch:** `feat/admin-portal/compliance-upload-on-behalf`

**Backend contract** (implemented separately in R2; do not wait for it):
- `POST ${BASE}/api/v1/compliance/admin/profiles/:profile_id/documents`
- Body JSON: `{ document_type_id: string, document_number: string, file_base64: string, content_type: string, issue_date?: "YYYY-MM-DD", expiry_date?: "YYYY-MM-DD" }`
- Responses:
  - `201 { data: DriverDocument }`
  - `403` without `compliance:admin`
  - `404` for an unknown or other-tenant profile
  - `4xx { error/message }` for validation

**Requirements:**
1. **API helper.** In `apps/admin-portal/src/lib/api/compliance.ts`, add `uploadDocumentOnBehalf(profileId, input)` using the file's existing `authFetch` + `okJson` pattern and `BASE`.
2. **Button.** In `components/compliance/document-detail-panel.tsx`, add an **"Upload document"** button, rendered only when the parent passes `canAdmin` (the page already computes `hasPermission("compliance:admin")`). Pass it down the same way `onSuspend` is.
3. **Modal.** The button opens a modal, using the portal's existing dialog/modal component. Fields:
   - document type: select fed by the existing `fetchDocumentTypes`;
   - document number: required, ≤ 100 characters;
   - issue date and expiry date: optional;
   - file: `accept="image/jpeg,image/png,image/webp,application/pdf"`.

   Behaviour:
   - Reject files over 10 MB client-side with a clear message.
   - Read the file as base64 with `FileReader`, stripping the `data:*;base64,` prefix.
   - Submit with the busy state disabled.
   - On success, close the modal and refresh the panel the way approve/reject already do (`refreshKey` / reload).
   - On error, show the server's message inline.
4. **Style.** Match the surrounding panel's existing styles (dark glass, existing button variants). Add no new libraries.
5. **Gates:** the frontend gates for admin-portal.

### Task W3: merchant-portal marketplace — real backend instead of localStorage

**Branch:** `feat/portals/marketplace-real-backend` (shared with W4)

**Backend contract** (carrier service through the gateway, `API_BASE = NEXT_PUBLIC_API_URL`, `authFetch`). All routes require `marketplace:book`.
- **Browse:** `GET /v1/marketplace/available?min_weight_kg=&size_class=&limit=` → `{ listings: VehicleListing[], count }`
- **Create:** `POST /v1/marketplace/bookings` with `{ listing_id, shipment_id, awb, consumer_name, consumer_phone?, pickup_label, dropoff_label, cargo_weight_kg, cargo_volume_m3?, distance_km, pickup_at (RFC3339), payment_method: "invoice"|"online" }` → `201 { booking: MarketplaceBooking, checkout_url: string|null }`
- **Mine:** `GET /v1/marketplace/my-bookings?limit=50&offset=0` → `{ bookings: MarketplaceBooking[], count }`
- **Cancel** (implemented separately in R9; do not wait): `POST /v1/marketplace/bookings/:id/cancel` → `{ booking }`. Only `pending` bookings.

**Shapes:**
- `VehicleListing`: `id, tenant_id, carrier_id, vehicle_plate, size_class, max_weight_kg, max_volume_m3?, base_price_cents, per_km_cents, per_kg_cents?, service_area_label, idle_from, idle_until, status, carrier_response_window_mins, bookings_today, revenue_today_cents, created_at, updated_at`
- `MarketplaceBooking`: `id, tenant_id, listing_id, carrier_id, shipment_id, awb, consumer_name, consumer_phone?, pickup_label, dropoff_label, cargo_weight_kg, cargo_volume_m3?, quoted_price_cents, status, pickup_at, picked_up_at?, picked_up_by?, pickup_notes?, booked_by_user_id?, payment_method, payment_status, payment_intent_id?, payment_checkout_url?, payment_authorized_at?, response_window_mins, distance_km, created_at, updated_at`
- Booking status values: `pending | accepted | rejected | in_transit | delivered | cancelled | disputed`
- Backend `size_class` values: `motorcycle | sedan | van | l300 | 6wheeler | 10wheeler | trailer`

**Requirements:**
1. **Rewrite `apps/merchant-portal/src/lib/api/marketplace.ts`** against the contract above:
   - remove `MOCK_LISTINGS`, `MOCK_BOOKINGS`, `CURRENT_MERCHANT_ID`/`NAME` and every `marketplace-bus` import;
   - the frontend types mirror the backend shapes exactly;
   - size classes are exactly the backend set (relabel `SIZE_CLASS_LABEL`);
   - stats are computed from the real bookings;
   - `subscribeToMarketplaceUpdates` becomes a polling refresh of my-bookings at 30s, or is removed with its callers. Choose the smaller change and say which.
2. **Booking needs a real shipment.** The booking flow makes the merchant **pick one of their own shipments** (use merchant-portal's existing shipments API/list). That supplies `shipment_id` and `awb` and prefills `pickup_label`, `dropoff_label` and `cargo_weight_kg` where the shipment has them. Other fields:
   - `distance_km` is entered or left 0;
   - `payment_method` is a choice, default `invoice`;
   - when `checkout_url` is non-null, `window.location.assign(checkout_url)`.
3. **Cancel.** A Cancel action on `pending` bookings calls the cancel route and refreshes.
4. **Receipt modal.** `ReceiptModal` is driven from a `MarketplaceBooking` (no `BusReceipt`). Show only what the booking actually holds.
5. **Delete** `apps/merchant-portal/src/lib/api/marketplace-bus.ts`.
6. **Errors.** Show the server's message for 4xx (BusinessRule texts are user-facing).
7. **Gates:** the frontend gates for merchant-portal, **including `npm run build`**.

### Task W4: partner and admin portals — drop the marketplace localStorage bus

**Branch:** `feat/portals/marketplace-real-backend`

**Bus consumers today:**
- **partner-portal**
  - `lib/api/marketplace.ts`: `appendReceipt`, `findReceiptByBookingId`, `subscribeToBus`, `BusReceipt`
  - `app/(dashboard)/marketplace/page.tsx`: `findReceiptByBookingId`
  - `components/marketplace/ReceiptModal.tsx`: types
- **admin-portal**
  - `lib/api/marketplace.ts`: `readBus`, `subscribeToBus`, `findReceiptByBookingId`
  - `app/(dashboard)/marketplace/page.tsx`
  - `app/(dashboard)/dispatch/page.tsx`: `readBus`, `subscribeToBus`, `BusBooking`
  - `components/marketplace/ReceiptModal.tsx`

**Requirements:**
1. Receipts in both portals are derived from the real `MarketplaceBooking` each portal already fetches. Same shape as W3; reuse the W3 type shape, but each portal keeps its own copy because the portals do not share a package today.
2. **Admin dispatch page.**
   - Source marketplace bookings from the closest real admin endpoint that admin-portal's `lib/api/marketplace.ts` already calls.
   - **If no real endpoint can supply what a bus consumer shows, remove that UI element's bus dependency and report it.** Do not invent a backend endpoint, and do not keep the bus.
3. Delete both `marketplace-bus.ts` files. `grep -rn marketplace-bus apps/` must return nothing.
4. **Gates:** the frontend gates for partner-portal and admin-portal, including `npm run build` for both.
