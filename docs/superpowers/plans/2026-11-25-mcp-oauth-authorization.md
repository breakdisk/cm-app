# Remote MCP OAuth 2.1 Authorization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Standard MCP clients can connect to `/mcp` through a consent flow, without a pasted portal token. Every client token is audience-bound, scope-narrowed and revocable.

**Architecture:**
- **Identity** becomes an OAuth 2.1 authorization server for the single resource `/mcp`. It issues ES256 access tokens under a key no other service accepts.
- **ai-layer** verifies those tokens through JWKS. It exchanges each one at identity for a short-lived HS256 platform token, with permissions = current roles ∩ granted scopes, and runs R1's unchanged `mcp_tool_access` against it.
- **The gateway** routes the discovery and OAuth endpoints publicly and rate-limits `/mcp` by tenant and client.

**Tech Stack:**
- **Rust:** axum, sqlx, jsonwebtoken 9 (ES256, `jwk`), reqwest (rustls), Redis.
- **Landing app:** Next.js 14 App Router.
- **Admin portal:** Next.js 14.

**Spec:** [ADR-0019](../../adr/0019-remote-mcp-authorization-oauth.md). The ADR is the authority; section numbers below (§N) refer to it.

## Prerequisites (block Task 1)

1. **R1 merged.** Branch `fix/ai-layer/mcp-per-tool-rbac` (per-tool RBAC, `MCP_TOOL_RULES`, `mcp_tool_access`, bearer forwarding through `ToolContext`) must be on `master`. Tasks 2, 11 and 12 build on its names.
2. **The ADR is Accepted,** with its three open questions answered. This plan assumes the proposals: DCR off by default, 15 min access token, 60 s exchange cache, no `client_credentials`.
3. **There is a public HTTPS origin** for `OAUTH__ISSUER` and `OAUTH__RESOURCE`. Without one, only Tasks 1–10 and 13 can be tested, locally against `http://localhost`.

## Global Constraints

- **Feature flag.** Everything ships behind `OAUTH__ENABLED` (identity, ai-layer, gateway), default `false`. When false, every `/oauth/*` and `/.well-known/oauth-*` route returns `404`, and `/mcp` behaves exactly as today.
- **Configuration keys.** Use these names verbatim:
  - `OAUTH__ISSUER`
  - `OAUTH__RESOURCE`
  - `OAUTH__SIGNING_KEY_PEM` (PKCS#8 EC P-256)
  - `OAUTH__SIGNING_KID`
  - `OAUTH__PREVIOUS_PUBLIC_KEY_PEM`, `OAUTH__PREVIOUS_KID` (both optional)
  - `OAUTH__DCR_ENABLED` (default `false`)
  - `MCP__ACCEPT_PLATFORM_TOKENS` (default `true`)
  - **Secrets** come from Vault-backed env at deploy. Never commit them to env files.
- **Lifetimes:**

  | Item | Lifetime |
  |---|---|
  | MCP access token | 900 s |
  | Exchanged platform token | 300 s |
  | Exchange cache | 60 s per `jti` |
  | Authorization code | 60 s, single use |
  | Pending authorization | 600 s |
  | Refresh token | 30 d sliding, 90 d absolute from grant creation |
  | Refresh reuse grace | 30 s |
  | CIMD cache ceiling | 24 h |
  | JWKS cache | 600 s, unknown-`kid` refetch at most once per 60 s |
- **Scopes,** exactly three: `logisticos.read`, `logisticos.operate`, `logisticos.billing`, with the permission sets in ADR §2.
- **Token and code storage.** Codes and refresh tokens are 32 random bytes, base64url, stored only as SHA-256 hex. Comparisons are constant-time.
- **No passthrough.** ai-layer never sends an MCP access token to any service except identity's exchange route.
- **Migrations:** identity `0019_oauth.sql`. Additive, `IF NOT EXISTS`, schema-isolated (ADR-0012).
- **Rust gates.** Run with `CARGO_INCREMENTAL=0`: `cargo check -p <crate>`, `cargo clippy -p <crate> --all-targets -- -D warnings`, `cargo test -p <crate>`. Zero `unwrap()` outside tests.
- **Frontend gates.** Delete `tsconfig.tsbuildinfo`, then run `npx tsc --noEmit`, `npx next lint` (an `Error:` fails), and `npm run build`.
- **Accessibility.** WCAG 2.1 AA on the consent page and the admin page.
- **OAuth error bodies** follow RFC 6749 §5.2 (`{"error": "...", "error_description": "..."}`). Error codes: `invalid_request`, `invalid_client`, `invalid_grant`, `unauthorized_client`, `unsupported_grant_type`, `invalid_scope`, `access_denied`, `invalid_target` (RFC 8707).

## Review Focus

1. **A client retries a refresh after a network timeout,** presenting the token it just rotated. Expected: within 30 s it gets a fresh successor (the earlier successor is revoked), not a revoked grant. After 30 s, the same presentation is reuse and revokes the family. (Task 7)
2. **Redirect URI near-misses:**
   - a trailing slash, an added query string, or a different host case is rejected;
   - `http://127.0.0.1:<any port>/cb`, `http://[::1]:<any port>/cb` and `http://localhost:<any port>/cb` match a registered loopback URI with any port;
   - `https://` URIs must match exactly. (Task 5)
3. **The user's role is reduced, or the tenant loses `enterprise_mcp`, after consent.** Expected: the next exchange (≤ 60 s later) reflects the new permissions or refuses, and refresh refuses for a tenant without the feature. (Task 8)
4. **Signing-key rotation.** A token signed with the new `kid` reaches ai-layer before its JWKS cache refreshes. Expected: one refetch, then success. A flood of random `kid`s causes at most one fetch per 60 s. (Task 11)
5. **`resource` with a trailing slash** (`https://api.x/mcp/`). Expected: accepted as the same resource; a single trailing slash is stripped before comparison. Any other difference gives `invalid_target`. (Tasks 6 and 7)

---

## Phase 1 — Shared token library

### Task 1: `libs/auth` OAuth token types, signer, verifier, scope math

**Files:**
- Create: `libs/auth/src/oauth.rs`
- Modify: `libs/auth/src/lib.rs` (`pub mod oauth;`), `libs/auth/src/claims.rs` (two optional fields)
- Test: `libs/auth/src/oauth.rs` (`#[cfg(test)] mod tests`)

**Interfaces — Produces:**
- `pub const SCOPES: [&str; 3] = ["logisticos.read", "logisticos.operate", "logisticos.billing"];`
- `pub fn scope_permissions(scope: &str) -> Option<&'static [&'static str]>`
- `pub fn parse_scopes(s: &str) -> Result<Vec<&'static str>, OAuthError>`. Space-separated; an unknown scope is `invalid_scope`; duplicates collapse.
- `pub fn effective_permissions(user_permissions: &[String], scopes: &[&str]) -> Vec<String>`. Returns user ∩ union(scope sets). A user holding `*` gets the full union. The result is sorted and deduplicated.
- `pub struct McpAccessClaims { iss: String, aud: String, sub: String, tenant_id: Uuid, client_id: String, grant_id: Uuid, scope: String, jti: String, iat: i64, exp: i64 }`, with Serialize/Deserialize.
- `pub struct McpTokenSigner`: `fn from_pem(pem: &str, kid: &str) -> Result<Self, OAuthError>` and `fn sign(&self, claims: &McpAccessClaims) -> Result<String, OAuthError>` (ES256, header `kid`).
- `pub struct McpTokenVerifier`: `fn new(issuer: &str, resource: &str, keys: JwkSet) -> Self`, `fn verify(&self, token: &str) -> Result<McpAccessClaims, OAuthError>`, and `fn kid_of(token: &str) -> Option<String>`.
- `pub fn public_jwk(pem: &str, kid: &str) -> Result<jsonwebtoken::jwk::Jwk, OAuthError>`
- `pub fn normalize_resource(s: &str) -> &str`. Strips one trailing `/`.
- `pub enum OAuthError`, with `fn code(&self) -> &'static str` returning the RFC error code.
- `Claims` gains `#[serde(default)] pub mcp_client_id: Option<String>` and `#[serde(default)] pub mcp_grant_id: Option<Uuid>`. Fix every `Claims { .. }` literal in the workspace.

- [ ] **Step 1: Write the failing tests.** Generate the test key in-test with `p256`, or use a fixed PKCS#8 test PEM in a `const`.
  - `sign_then_verify_roundtrip`: claims come back equal.
  - `verify_rejects_wrong_audience`: `aud = "https://other/mcp"` → `Err`.
  - `verify_rejects_wrong_issuer`, `verify_rejects_expired` (exp = now − 120, beyond the 60 s leeway).
  - `verify_rejects_unknown_kid`: `Err`, and `kid_of` returns the kid.
  - `platform_validator_refuses_mcp_token`: `JwtService::new(secret, ..).validate_access_token(&es256_token)` is `Err`. **This is the ADR's load-bearing invariant; keep the test name.**
  - `effective_permissions_never_widens`: a user with `["shipments:read"]`, scopes read+operate → `["shipments:read"]`.
  - `effective_permissions_wildcard_gets_scope_union`: a user with `["*"]`, scope billing → `["payments:reconcile"]`.
  - `parse_scopes_rejects_unknown`: `"logisticos.read admin"` → `invalid_scope`.
  - `normalize_resource_strips_one_slash`: `"https://a/mcp/"` → `"https://a/mcp"`; `"https://a/mcp//"` → `"https://a/mcp/"`.
- [ ] **Step 2: Run them and confirm they fail.** `CARGO_INCREMENTAL=0 cargo test -p logisticos-auth oauth` fails to compile (module missing).
- [ ] **Step 3: Implement `oauth.rs`.** The scope sets are the ADR §2 strings, written as `permissions::*` constants. Fix the `Claims` literals.
- [ ] **Step 4: Run the gates.** `cargo test -p logisticos-auth` passes, then `cargo clippy -p logisticos-auth --all-targets -- -D warnings`, then `cargo check --workspace`, which catches `Claims` literals in other crates.
- [ ] **Step 5: Commit** `feat(auth): MCP access tokens on their own ES256 key, and scope math`.

### Task 2: ai-layer coverage test — every exposed tool's permission is in exactly one scope

**Files:**
- Modify: `services/ai-layer/src/api/mcp/mod.rs` (test module, plus one fn)

**Interfaces:**
- **Consumes:** `MCP_TOOL_RULES` (R1) and `scope_permissions`/`SCOPES` (Task 1).
- **Produces:** `pub fn required_scope_for_tool(tool: &str) -> Option<&'static str>`. It returns the scope containing the tool's `any_of[0]`. `None` means the tool is not exposed.

- [ ] **Step 1: Write the test** `every_rule_permission_in_exactly_one_scope`. For each rule and each `p` in `any_of`, the count of `SCOPES` whose set contains `p` == 1. Also write `required_scope_examples`: `get_shipment` → read, `assign_driver` → operate, `generate_invoice` → billing, `cancel_shipment` → `None`.
- [ ] **Step 2: Run it and confirm it fails** (fn missing).
- [ ] **Step 3: Implement `required_scope_for_tool`.**
- [ ] **Step 4: Run the tests.** `cargo test -p logisticos-ai-layer mcp::` passes. Run clippy.
- [ ] **Step 5: Commit** `test(ai-layer): every MCP tool permission lands in exactly one OAuth scope`.

## Phase 2 — Identity as authorization server

### Task 3: OAuth storage

**Files:**
- Create: `services/identity/migrations/0019_oauth.sql`, `services/identity/src/infrastructure/db/oauth_repo.rs`
- Modify: `services/identity/src/infrastructure/db/mod.rs`
- Test: `services/identity/tests/integration/oauth_repo_db.rs`. Same harness as `partner_membership_db.rs`; the test must **fail, not skip**, when its database env var is missing in CI.

**Tables** (schema `identity`, all `TIMESTAMPTZ`):
- `oauth_clients (client_id TEXT PK, kind TEXT CHECK (kind IN ('cimd','preregistered','dcr')), tenant_id UUID NULL, client_name TEXT, logo_uri TEXT NULL, redirect_uris TEXT[] NOT NULL, secret_hash TEXT NULL, verified BOOL NOT NULL DEFAULT false, metadata_fetched_at TIMESTAMPTZ NULL, created_at, last_used_at NULL, revoked_at NULL)`
- `oauth_pending_authorizations (id UUID PK, client_id, redirect_uri, scopes TEXT[], code_challenge TEXT, resource TEXT, state TEXT NULL, expires_at, created_at)`
- `oauth_grants (id UUID PK, tenant_id, user_id, client_id, scopes TEXT[], created_at, last_used_at NULL, revoked_at NULL)`, with a partial index on `(tenant_id) WHERE revoked_at IS NULL`.
- `oauth_auth_codes (code_hash TEXT PK, grant_id, redirect_uri, code_challenge, resource, expires_at, used_at NULL)`
- `oauth_refresh_tokens (token_hash TEXT PK, grant_id, family_id UUID, successor_hash TEXT NULL, rotated_at NULL, revoked_at NULL, expires_at, created_at)`, with an index on `(family_id)`.

**Interfaces — Produces** `PgOAuthRepository` with these methods:
- `upsert_client`, `find_client`
- `insert_pending`, `take_pending(id) -> Option<Pending>`. Deletes and returns; returns `None` if expired.
- `insert_grant`, `find_active_grant`, `revoke_grant(id)`. Revoking also revokes every refresh token of the grant.
- `insert_code`, `consume_code(hash) -> Option<Code>`. A single `UPDATE … SET used_at = now() WHERE code_hash=$1 AND used_at IS NULL AND expires_at > now() RETURNING …`.
- `insert_refresh`, `rotate_refresh(hash, new_hash) -> RotateOutcome { Rotated(Grant), Grace(Grant), Reuse, Invalid }`. Grace applies within 30 s of `rotated_at`. The caller revokes the family on `Reuse`.
- `list_grants(tenant_id, user_id: Option<Uuid>)`, `touch_grant(id)`.

- [ ] **Step 1: Write the integration tests:**
  - `code_is_single_use`: the second `consume_code` → `None`.
  - `expired_code_is_refused`
  - `rotate_then_reuse_within_grace_is_grace`
  - `rotate_then_reuse_after_grace_is_reuse`. Set `rotated_at` back 31 s by SQL.
  - `revoke_grant_revokes_refresh_family`
- [ ] **Step 2: Run them and confirm they fail.**
- [ ] **Step 3: Write the migration and the repository.**
- [ ] **Step 4: Run the tests.** `cargo test -p logisticos-identity --test integration oauth_repo` passes, with the database env var set.
- [ ] **Step 5: Commit** `feat(identity): OAuth client, grant, code and refresh-token storage`.

### Task 4: Signing keys, JWKS and authorization-server metadata

**Files:**
- Create: `services/identity/src/api/http/oauth/mod.rs` (router), `services/identity/src/api/http/oauth/discovery.rs`, `services/identity/src/config/oauth.rs` (or the crate's config pattern)
- Modify: `services/identity/src/api/http/mod.rs` (merge the oauth router when `OAUTH__ENABLED`)

**Interfaces — Produces:**
- `OAuthConfig { enabled, issuer, resource, signer: McpTokenSigner, jwks: JwkSet, dcr_enabled }`, held in `AppState`.
- `GET /.well-known/jwks.json` returns the current key plus the previous key when configured.
- `GET /.well-known/oauth-authorization-server` returns the ADR §3 document. `registration_endpoint` is present only when `dcr_enabled`.

- [ ] **Step 1: Write the tests.**
  - `metadata_lists_s256_only`: `code_challenge_methods_supported == ["S256"]`.
  - `metadata_omits_registration_when_dcr_off`
  - `jwks_contains_previous_key_when_configured`: two keys with distinct `kid`s.
  - `disabled_returns_404`: with `enabled=false` the router is absent.

  Use `tower::ServiceExt::oneshot` against the router.
- [ ] **Step 2: Run them and confirm they fail.**
- [ ] **Step 3: Implement.** A missing or invalid PEM while enabled is a startup error naming the env var. It is not a panic in a handler.
- [ ] **Step 4: Run the identity gates.**
- [ ] **Step 5: Commit** `feat(identity): OAuth discovery metadata and JWKS`.

### Task 5: Client resolution — CIMD fetch, pre-registered lookup, redirect matching

**Files:**
- Create: `services/identity/src/application/services/oauth_clients.rs`, `services/identity/src/infrastructure/external/client_metadata.rs`

**Interfaces — Produces:**
- `pub fn redirect_uri_matches(registered: &[String], presented: &str) -> bool`. Exact string match, except where a registered URI's host is `127.0.0.1`, `[::1]` or `localhost` with scheme `http`: then any port matches, as long as scheme, host and path match exactly.
- `pub fn is_forbidden_ip(ip: IpAddr) -> bool`. Covers loopback, private (10/8, 172.16/12, 192.168/16), link-local (169.254/16, fe80::/10), unique-local fc00::/7, unspecified, multicast and broadcast, and IPv4-mapped forms of all of these.
- `pub async fn fetch_client_metadata(url: &Url) -> Result<ClientMetadata, OAuthError>`:
  - https only;
  - resolve DNS first and reject if any address `is_forbidden_ip`, then connect to the resolved address (reqwest `resolve`) so DNS rebinding cannot slip in;
  - 5 s timeout, 64 KB cap, no cross-host redirects;
  - the document's `client_id` must equal the URL.
- `pub async fn resolve_client(repo, client_id: &str) -> Result<ResolvedClient, OAuthError>`:
  - `client_id` starts with `https://`: a CIMD fetch, cached in `oauth_clients` with `kind='cimd'` and refreshed after 24 h;
  - otherwise: a lookup with `kind IN ('preregistered','dcr')`, not revoked.

- [ ] **Step 1: Write the tests.**
  - `redirect_exact_https_only`: a trailing slash, an added `?x=1` and an upper-case host each → false.
  - `redirect_loopback_any_port`: registered `http://127.0.0.1/cb`, presented `http://127.0.0.1:53117/cb` → true; presented `http://127.0.0.1:53117/other` → false.
  - `forbidden_ip_table`: one assert per range above, plus `8.8.8.8` → false.
  - `cimd_rejects_http_scheme`
  - `cimd_rejects_mismatched_client_id`: serve a doc from a local test server and call the parser directly. The network guard is covered by `forbidden_ip_table`.
- [ ] **Step 2: Run them and confirm they fail.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run the identity gates.**
- [ ] **Step 5: Commit** `feat(identity): OAuth client resolution with SSRF-hardened metadata fetch`.

### Task 6: Authorize endpoint and consent API

**Files:**
- Create: `services/identity/src/api/http/oauth/authorize.rs`, `services/identity/src/application/services/oauth_authorize.rs`

**Interfaces:**
- **Consumes:** Tasks 1, 3 and 5.
- **Produces:**
  - `GET /oauth/authorize` (public). It validates via `validate_authorize_request(params, client) -> Result<ValidRequest, AuthorizeError>`.
    - **Errors before the client and redirect are trusted** are shown as an HTML error page, never redirected.
    - **Errors after** redirect with `error=` and `state`.
    - **On success** it inserts a pending authorization and returns `302` to `<landing>/login?returnTo=/oauth/consent?req=<id>`. The landing origin is config `OAUTH__CONSENT_ORIGIN`.
  - `GET /v1/oauth/consent/:req` (platform JWT) returns `{ client_name, logo_uri, redirect_host, verified, tenant_name, scopes: [{scope, description}], refusal: null | "feature_missing" | "no_permissions" }`.
  - `POST /v1/oauth/consent/:req` (platform JWT) takes `{ approve: bool }` and returns `{ redirect_to }`.
    - **Approve:** `take_pending`, refuse on `refusal`, `insert_grant` for `claims.user_id` / `claims.tenant_id`, `insert_code` (60 s). `redirect_to = redirect_uri?code=…&state=…&iss=<issuer>`.
    - **Deny:** `redirect_to` carries `error=access_denied`.
- **Refusal:** `feature_missing` when the tenant's features (from the DB, not the token) lack `enterprise_mcp`. `no_permissions` when `effective_permissions(user_perms, requested_scopes)` is empty.

- [ ] **Step 1: Write the unit tests** for `validate_authorize_request`:
  - `requires_pkce_s256`: a missing `code_challenge` and `code_challenge_method=plain` each → `invalid_request`.
  - `requires_resource`: missing → `invalid_request`; a different resource → `invalid_target`; a trailing slash → ok.
  - `rejects_unregistered_redirect_without_redirecting`: the error variant is `Untrusted`.
  - `rejects_unknown_scope`: `invalid_scope`, redirected.
  - `defaults_scope_to_read_when_absent`.

  Then the handler tests: `consent_refuses_tenant_without_enterprise_mcp`, `approve_issues_single_use_code`, `deny_redirects_access_denied`.
- [ ] **Step 2: Run them and confirm they fail.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run the identity gates.**
- [ ] **Step 5: Commit** `feat(identity): OAuth authorize endpoint and consent API`.

### Task 7: Token endpoint

**Files:**
- Create: `services/identity/src/api/http/oauth/token.rs`, `services/identity/src/application/services/oauth_token.rs`

**Interfaces — Produces:**
- `pub fn verify_pkce_s256(verifier: &str, challenge: &str) -> bool`. Checks the verifier length is 43–128 and the charset is the unreserved set.
- `POST /oauth/token` (form-encoded, public). Responses carry `Cache-Control: no-store`.
  - **`authorization_code`:** check `consume_code`, `redirect_uri` equality, `client_id` equality, PKCE, and `normalize_resource(resource)` equality. Then sign an `McpAccessClaims` (900 s, `jti` = UUIDv4) and issue a refresh token in a new family. Response: `{ access_token, token_type: "Bearer", expires_in: 900, refresh_token, scope }`.
  - **`refresh_token`:** `rotate_refresh`.
    - `Rotated` / `Grace`: issue a new pair.
    - `Reuse`: revoke the family, then `invalid_grant`.
    - Re-check that the tenant still has `enterprise_mcp` and the grant is active; refuse with `invalid_grant` otherwise.
    - Enforce the 90 d absolute limit from `grant.created_at`.
  - **Confidential clients** (`secret_hash` set) authenticate with `client_secret_basic`, or get `invalid_client` with a `401`.

- [ ] **Step 1: Write the tests:**
  - `pkce_rfc7636_appendix_b_vector`: verifier `dBjftJeZ4CVP-mJ92K1quvH6Fqyz5GaIhfVQ1Wr0kRk` gives challenge `E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM` → true; one changed character → false.
  - `code_exchange_happy_path_returns_verifiable_token`: `McpTokenVerifier` accepts it, `aud == resource`.
  - `code_reuse_is_invalid_grant`
  - `wrong_redirect_uri_is_invalid_grant`
  - `refresh_retry_within_grace_succeeds` (Review Focus 1)
  - `refresh_reuse_after_grace_revokes_family`: a later refresh with the newest token also fails.
  - `refresh_refused_after_enterprise_downgrade` (Review Focus 3)
- [ ] **Step 2: Run them and confirm they fail.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run the identity gates.**
- [ ] **Step 5: Commit** `feat(identity): OAuth token endpoint with PKCE and refresh rotation`.

### Task 8: Revocation and the internal token exchange

**Files:**
- Create: `services/identity/src/api/http/oauth/exchange.rs`, `services/identity/src/api/http/oauth/revoke.rs`
- Modify: `services/identity/src/api/http/mod.rs`. Add `/v1/internal/oauth/exchange` to `internal_router` (guarded by `require_internal_secret`).

**Interfaces — Produces:**
- `POST /oauth/revoke` (RFC 7009). Takes a refresh token and revokes its family. Always returns `200`, including for unknown tokens.
- `POST /v1/internal/oauth/exchange`. Body `{ subject_token, subject_token_type: "urn:ietf:params:oauth:token-type:access_token" }`. Response `{ access_token, issued_token_type: "urn:ietf:params:oauth:token-type:access_token", token_type: "Bearer", expires_in: 300 }`. Steps:
  1. Verify the subject with the identity-side `McpTokenVerifier`.
  2. Load the grant (active), the user (active), roles → permissions, and tenant features.
  3. Refuse with `invalid_grant` on any failure, or when `enterprise_mcp` is missing.
  4. Mint the platform `Claims` with the user's normal claim set, but `permissions = effective_permissions(..)`, `exp = now + 300`, `mcp_client_id`, `mcp_grant_id`.
  5. `touch_grant`.

- [ ] **Step 1: Write the tests:**
  - `exchange_narrows_permissions_to_scopes`
  - `exchange_reflects_role_reduction` (Review Focus 3)
  - `exchange_refuses_revoked_grant`
  - `exchange_refuses_without_internal_secret`: `401`.
  - `exchange_output_validates_as_platform_token_and_input_does_not`
  - `revoke_unknown_token_is_200`
- [ ] **Step 2: Run them and confirm they fail.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run the identity gates.**
- [ ] **Step 5: Commit** `feat(identity): OAuth revocation and internal token exchange`.

### Task 9: Dynamic Client Registration (flagged off)

**Files:**
- Create: `services/identity/src/api/http/oauth/register.rs`

**Interfaces — Produces:** `POST /oauth/register` (RFC 7591), mounted only when `OAUTH__DCR_ENABLED`.
- It accepts `client_name`, `redirect_uris`, `logo_uri?` and `token_endpoint_auth_method: "none"` only, and stores `kind='dcr'`, `verified=false`.
- Redirect URIs must be `https`, or `http` on a loopback host.
- Rate limit: 10 registrations per IP per hour, in Redis key `oauth:dcr:<ip>`.
- A daily sweep deletes `dcr` clients with no grant after 30 days.

- [ ] **Step 1: Write the tests:** `register_rejects_http_non_loopback`, `register_rejects_auth_method_secret`, `register_absent_when_flag_off`, and `sweep_deletes_unused_after_30_days` (pure decision fn `should_expire(created_at, has_grant, now)`).
- [ ] **Step 2: Run them and confirm they fail.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run the identity gates.**
- [ ] **Step 5: Commit** `feat(identity): dynamic client registration behind OAUTH__DCR_ENABLED`.

### Task 10: Landing consent page

**Files:**
- Create: `apps/landing/src/app/oauth/consent/page.tsx`, `apps/landing/src/lib/oauth.ts`

**Interfaces:**
- **Consumes:** Task 6's consent API, through the landing app's existing authenticated fetch.
- **Produces** the route `/oauth/consent?req=<id>`:
  - **Not signed in:** `router.replace('/login?returnTo=' + encodeURIComponent(currentPathAndQuery))`.
  - **Signed in:** show the client logo and name, `redirect_host`, an "Unverified application" banner when `!verified`, the tenant name, and one plain-language line per scope (copy below). Approve and Deny buttons; then `window.location.assign(redirect_to)`.
  - **`refusal` set:** no Approve button, plus the message for that refusal.

**Copy** (exact; follow the app's existing i18n mechanism if it has one, else inline EN):

| Key | Text |
|---|---|
| `logisticos.read` | "Read your shipments, drivers, fleet, customers, analytics and COD balances" |
| `logisticos.operate` | "Reschedule deliveries, assign drivers, message drivers and send customer notifications" |
| `logisticos.billing` | "Generate invoices and reconcile cash-on-delivery" |
| `feature_missing` | "AI client connections are available on the Enterprise plan." |
| `no_permissions` | "Your role does not allow any of the access this application asks for." |

- [ ] **Step 1: Write the jest tests** for the pure helpers in `lib/oauth.ts`: `consentLoginRedirect(pathWithQuery)` encodes correctly, and `scopeLine(scope)` returns the copy above, or `null` for an unknown scope.
- [ ] **Step 2: Run them and confirm they fail.**
- [ ] **Step 3: Implement the page.** Use the landing design tokens. Approve is the focused default only after the page has loaded. Buttons are disabled while a request is in flight.
- [ ] **Step 4: Run the landing frontend gates.**
- [ ] **Step 5: Commit** `feat(landing): OAuth consent page`.

## Phase 3 — Resource server and gateway

### Task 11: ai-layer `mcp_auth`, JWKS cache, token exchange, Protected Resource Metadata

**Files:**
- Create: `services/ai-layer/src/api/mcp/auth.rs`, `services/ai-layer/src/infrastructure/identity_oauth.rs`
- Modify: `services/ai-layer/src/bootstrap.rs`. Move `/mcp` out from under `require_auth` and wrap it in `mcp_auth`, and add the PRM routes.

**Interfaces:**
- **Consumes:** Tasks 1, 2 and 8.
- **Produces:**
  - `JwksCache::new(identity_url, ttl=600s, min_refetch=60s)` and `async fn verifier(&self, kid: &str) -> Result<McpTokenVerifier, OAuthError>`.
  - `ExchangeClient::exchange(&self, mcp_token: &str, jti: &str) -> Result<Claims, OAuthError>`. It is cached per `jti` for min(60 s, token remaining) in a bounded map (10 000 entries, evicting the oldest).
  - The `mcp_auth` middleware:
    1. Take the bearer.
    2. With `OAUTH__ENABLED` and an ES256 header, verify, exchange, and insert the `Claims` and an `McpGrantContext { client_id, grant_id, scopes }` into request extensions.
    3. Otherwise, if `MCP__ACCEPT_PLATFORM_TOKENS`, use today's HS256 validation and insert `McpGrantContext::bridge()`.
    4. Otherwise answer `401` with `WWW-Authenticate: Bearer resource_metadata="<origin(OAUTH__RESOURCE)>/.well-known/oauth-protected-resource/mcp", scope="logisticos.read"`.
    5. **Step-up:** for a JSON-RPC `tools/call` POST (buffer at most 1 MB; larger → `413`), when `required_scope_for_tool(name)` is not in the token's scopes, answer `403` with `WWW-Authenticate: Bearer error="insufficient_scope", scope="<scope>", resource_metadata="…"`.
  - `GET /.well-known/oauth-protected-resource` and `GET /.well-known/oauth-protected-resource/mcp` return the ADR §4 JSON.

- [ ] **Step 1: Write the tests** (router + `oneshot`; identity faked with an in-process axum server on `127.0.0.1:0`):
  - `no_token_401_carries_resource_metadata`: the header value matches exactly.
  - `platform_token_accepted_only_while_bridge_on`
  - `mcp_token_exchanged_claims_reach_tool_gate`: `tools/list` returns only tools within the scopes.
  - `insufficient_scope_403_names_scope`: a read-only token calling `assign_driver`.
  - `unknown_kid_triggers_single_refetch` (Review Focus 4): 50 random kids → the fake identity sees ≤ 1 JWKS fetch inside 60 s.
  - `exchange_cached_per_jti`: two calls → one exchange.
  - `mcp_token_never_forwarded`: the fake downstream records the bearer, and it equals the exchanged token.
- [ ] **Step 2: Run them and confirm they fail.**
- [ ] **Step 3: Implement.** `IDENTITY_URL` and `LOGISTICOS_INTERNAL_SECRET` follow the crate's existing service-URL config.
- [ ] **Step 4: Run the ai-layer gates.**
- [ ] **Step 5: Commit** `feat(ai-layer): /mcp as an OAuth resource server`.

### Task 12: Audit the grant on every remote call

**Files:**
- Modify: `services/ai-layer/src/api/mcp/mod.rs` (`record_audit`)

**Interfaces — Consumes:** `McpGrantContext` (Task 11). `AgentSession.trigger` gains `client_id`, `grant_id` and `bridge: bool`.

- [ ] **Step 1: Write the test** `audit_trigger_carries_client_and_grant`. Use an in-memory `SessionRepository` fake and assert the three keys, and that a bridge call has `bridge: true` and `client_id: null`.
- [ ] **Step 2: Run it and confirm it fails.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run the ai-layer gates.**
- [ ] **Step 5: Commit** `feat(ai-layer): audit the OAuth client and grant on remote MCP calls`.

### Task 13: Gateway routing, public paths, rate-limit keying

**Files:**
- Modify: `services/api-gateway/src/bootstrap.rs` (`is_public`, the `/mcp` branch), `services/api-gateway/src/proxy/mod.rs` (`resolve_upstream`, tests)

**Interfaces — Consumes:** `McpTokenVerifier` and `JwksCache`. Promote `JwksCache` to `libs/auth` behind a `jwks-client` feature if both crates need it. Decide in this task and record the decision.

- **Public** (exact or prefix as listed): `/.well-known/oauth-authorization-server`, `/.well-known/oauth-protected-resource` (prefix), `/.well-known/jwks.json`, `/oauth/authorize`, `/oauth/token`, `/oauth/revoke`, `/oauth/register`.
- **Routes:**
  - `/.well-known/oauth-protected-resource*` → ai-layer;
  - `/oauth/*`, `/.well-known/oauth-authorization-server` and `/.well-known/jwks.json` → identity;
  - `/v1/oauth/*` (consent and grants API) → identity.
- **`/mcp` with an ES256 bearer:** verify, and key the rate limit by `"{tenant_id}:{client_id}"` with the tenant's tier looked up as today. An invalid token is still forwarded, so ai-layer produces the spec-compliant `401`.
- **`/oauth/token` and `/oauth/register`:** per-IP limit of 60 requests per minute, Redis key `oauth:ip:<ip>`.

- [ ] **Step 1: Write the tests** in the existing `resolve` test style: each new path resolves to the right upstream; `is_public` holds for each listed path and not for `/oauth/anything-else`; `mcp_invalid_token_forwarded_not_401`.
- [ ] **Step 2: Run them and confirm they fail.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run the api-gateway gates.**
- [ ] **Step 5: Commit** `feat(api-gateway): route OAuth discovery and key /mcp limits by client`.

## Phase 4 — Management and rollout

### Task 14: Grants and pre-registered clients — API and admin page

**Files:**
- Create: `services/identity/src/api/http/oauth/grants.rs`, `apps/admin-portal/src/app/(dashboard)/settings/ai-clients/page.tsx`, `apps/admin-portal/src/lib/api/oauth.ts`
- Modify: the admin-portal settings navigation

**Interfaces — Produces:**
- `GET /v1/oauth/grants`:
  - with `API_KEYS_MANAGE`: every active grant in the tenant;
  - without it: the caller's own grants.
- `POST /v1/oauth/grants/:id/revoke`: the owner, or `API_KEYS_MANAGE`. A foreign tenant's grant → `404`.
- `POST /v1/oauth/clients` (`API_KEYS_MANAGE`): creates a `preregistered` client for the tenant. The response shows the secret once, for a confidential client only.
- `GET /v1/oauth/clients` (`API_KEYS_MANAGE`)
- The page has two tables: connected clients (client, user, scopes, created, last used, Revoke) and registered clients (name, redirect URIs, created, Create).
- Every create and revoke writes the identity audit log with actor, tenant and IP.

- [ ] **Step 1: Write the tests.**
  - **Identity:** `user_sees_only_own_grants`, `admin_sees_tenant_grants`, `revoke_foreign_tenant_grant_is_404`, `client_secret_returned_once_and_stored_hashed`.
  - **Admin portal jest:** the pure formatter for scope labels.
- [ ] **Step 2: Run them and confirm they fail.**
- [ ] **Step 3: Implement.** The Revoke confirm dialog meets the WCAG dialog rules: Escape, focus moved in, focus trapped, focus returned, `aria-labelledby`.
- [ ] **Step 4: Run the identity and admin-portal gates.**
- [ ] **Step 5: Commit** `feat(identity,admin-portal): manage OAuth grants and registered AI clients`.

### Task 15: Rollout runbook and conformance check

**Files:**
- Create: `docs/runbooks/mcp-oauth.md`

The runbook covers:
1. **Key generation.** `openssl ecparam -name prime256v1 -genkey -noout | openssl pkcs8 -topk8 -nocrypt`, then load it into Vault and set the `kid` as the date, e.g. `mcp-2026-12`.
2. **Enabling per environment,** in this order: identity → ai-layer → gateway.
3. **A conformance pass in staging.** Run an MCP client with OAuth support (MCP Inspector) against `OAUTH__RESOURCE`: discovery → consent → `tools/list` → a read tool → a step-up on an operate tool → revoke in the admin page → the next call fails within 60 s. Record the results in the runbook's verification log.
4. **Key rotation:** move the current key to `PREVIOUS_*`, install the new key, wait 900 s plus the JWKS TTL, then remove `PREVIOUS_*`.
5. **Bridge closure criteria:** zero `bridge=true` audit triggers over 14 days, then set `MCP__ACCEPT_PLATFORM_TOKENS=false`.
6. **The CISO sign-off and pen-test gate** before `OAUTH__ENABLED=true` in production.

- [ ] **Step 1: Write the runbook.**
- [ ] **Step 2: Run the staging conformance pass** and fill in the verification log. If there is no staging HTTPS origin, record that the pass is blocked and why.
- [ ] **Step 3: Commit** `docs(runbook): MCP OAuth rollout, rotation and bridge closure`.

## Not in this plan

- **`client_credentials`** for headless agents (ADR open question 3).
- **External identity-provider federation.**
- **Multi-tenant grants.**
