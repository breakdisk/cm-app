# ADR-0019: Remote MCP Authorization (OAuth 2.1)

**Status:** Proposed
**Date:** 2026-11-25
**Deciders:** Principal Architect, CISO, Staff ML Engineer / AI Architect, Senior Rust Engineer — Identity & Auth, EM — Platform Core
**Extends:** [ADR-0004](0004-mcp-for-ai-interoperability.md) (Enterprise Extension), [ADR-0011](0011-firebase-logisticos-jwt-bridge.md) (platform JWTs)

> **Load-bearing invariant.** A token issued to a third-party MCP client can do nothing except call `/mcp`. It is valid for one audience, signed with a key no other service accepts, and narrowed to the scopes the user consented to. Every downstream call runs on a short-lived platform token that identity mints, never on the client's token.

---

## Context

ai-layer serves a remote MCP server at `/mcp` (rmcp, stateless Streamable HTTP) over the shared `ToolRegistry`. Today:

- **The only credential is a platform JWT.** `/mcp` sits behind `require_auth` and accepts any LogisticOS access token from a tenant with the `enterprise_mcp` feature. A merchant who wants an MCP client to work must paste a portal token into it. That token expires with the portal session, carries every permission the user has, and cannot be revoked for one client without signing the user out.
- **Standard MCP clients cannot connect on their own.** The MCP authorization spec expects the server to answer `401` with a `WWW-Authenticate: Bearer resource_metadata="…"` challenge. It also expects Protected Resource Metadata (RFC 9728) pointing at an OAuth 2.1 authorization server, and that server's metadata (RFC 8414). LogisticOS serves none of these. Connector UIs such as Claude's custom connectors therefore have nothing to discover.
- **Platform tokens are valid everywhere.** `libs/auth` signs and checks every token with one HS256 secret that every service holds. `validate_access_token` checks signature and expiry but not audience. A token minted for any purpose works at every service. The gateway's diner-token comment already records this hazard. If identity minted "MCP tokens" with the same key, they would work at every service, and ai-layer passing them downstream would be exactly the token passthrough the MCP spec forbids.
- **Per-tool RBAC now exists.** `mcp_tool_access` in `services/ai-layer/src/api/mcp/mod.rs` is a single fail-closed gate used by both `list_tools` and `call_tool`. It maps each tool to one permission (branch `fix/ai-layer/mcp-per-tool-rbac`, plan task R1). OAuth scopes must narrow that gate, not replace it.
- **Most downstream handlers check only `require_auth`.** They check no permission. Narrowing a forwarded token's permissions is defence in depth where a handler checks, and nothing where it does not. The authoritative gate is the one in ai-layer.

## Decision

**Identity becomes an OAuth 2.1 authorization server for exactly one resource, `/mcp`. ai-layer becomes an OAuth resource server.** No third-party IdP is added.

### 1. Two kinds of token, two keys

| | Platform token (unchanged) | MCP access token (new) |
|---|---|---|
| Issuer | identity | identity |
| Algorithm, key | HS256, shared platform secret | **ES256**, a dedicated key pair in Vault, `kid` in the header |
| Accepted by | every service | **ai-layer `/mcp` only** (and the gateway, for rate-limit keying) |
| `aud` | absent | the canonical resource URL, `OAUTH__RESOURCE` (e.g. `https://api.<platform-domain>/mcp`) |
| Carries | tenant, roles, permissions, features | `sub`, `tenant_id`, `client_id`, `grant_id`, `scope`, `jti`, `iat`, `exp`. **No permissions.** |
| Lifetime | portal setting | 15 minutes |

The services' existing `Validation::new(Algorithm::HS256)` rejects an ES256 token on its header, so **no service needs a change to refuse MCP tokens**. Audience isolation comes from the key, and the `aud` claim makes it explicit for ai-layer. The public half of the key is published as JWKS. The current and previous key are listed to allow rotation.

### 2. Scopes narrow RBAC; they never widen it

There are three consent scopes. They are grouped so a person can understand them on a consent screen, not one per tool:

| Scope | Grants (as permission sets) | Example tools |
|---|---|---|
| `logisticos.read` | `shipments:read`, `dispatch:view`, `analytics:view`, `payments:read`, `customers:read`, `fleet:read` | `get_shipment`, `get_hub_capacity`, `get_cod_balance` |
| `logisticos.operate` | `shipments:update`, `dispatch:assign`, `engagement:send`, `fleet:manage` | `reschedule_delivery`, `assign_driver`, `send_driver_instruction` |
| `logisticos.billing` | `payments:reconcile` (both `BILLING_MANAGE` and `PAYMENTS_RECONCILE` resolve to it) | `generate_invoice`, `reconcile_cod` |

The permissions in force for a call are the user's **current** role permissions ∩ the union of the granted scopes' sets. Identity computes this at token exchange (§4). The permission strings are the `libs/auth/src/rbac.rs` constants that R1's `MCP_TOOL_RULES` names: 18 exposed tools, with `cancel_shipment` and the `MCP_HIDDEN` tools not exposed. The binding mapping is a `scope_permissions(scope)` function in `libs/auth`. A test asserts that every permission in every `MCP_TOOL_RULES` entry belongs to exactly one scope. A tool whose permission falls in no scope fails that test, so it cannot ship unreachable or silently widen a scope. Exposing a new tool means adding one rule and placing its permission in a scope, both reviewed together.

When a call needs a scope the token lacks, ai-layer answers `403` with `WWW-Authenticate: Bearer error="insufficient_scope", scope="<needed>", resource_metadata="…"`, so the client can ask the user to step up.

### 3. Authorization server endpoints (identity, routed by the gateway)

| Endpoint | Standard | Notes |
|---|---|---|
| `GET /.well-known/oauth-authorization-server` | RFC 8414 | `issuer = OAUTH__ISSUER`, `code_challenge_methods_supported: ["S256"]`, `response_types_supported: ["code"]`, `grant_types_supported: ["authorization_code","refresh_token"]`, `token_endpoint_auth_methods_supported: ["none","client_secret_basic"]`, `client_id_metadata_document_supported: true`, `registration_endpoint` only when DCR is enabled |
| `GET /.well-known/jwks.json` | RFC 7517 | current and previous ES256 public keys |
| `GET /oauth/authorize` | OAuth 2.1 | `response_type=code` only; **PKCE S256 required**; **`resource` required** and must equal `OAUTH__RESOURCE`; exact `redirect_uri` match (RFC 8252 loopback: any port on `127.0.0.1`/`[::1]` for native clients); `state` echoed |
| `POST /oauth/token` | OAuth 2.1, RFC 8707 | `authorization_code` (verifies PKCE and the `resource`/`redirect_uri`/`client_id` binding; single-use code, 60 s) and `refresh_token` (rotation on every use; reuse of a rotated token revokes the whole grant family) |
| `POST /oauth/revoke` | RFC 7009 | refresh tokens and grants |
| `POST /oauth/register` | RFC 7591 | Dynamic Client Registration, **behind `OAUTH__DCR_ENABLED`** (§6) |

**Client identification**, in order of preference:
1. **Client ID Metadata Documents.** The `client_id` is an HTTPS URL that identity fetches for name, logo, and redirect URIs.
   - The fetch is SSRF-hardened: https only, no private or link-local addresses after DNS resolution, 5 s timeout, 64 KB cap, no redirects to other hosts.
   - The result is cached per HTTP caching headers, with a 24 h ceiling.
2. **Pre-registered clients**, for an Enterprise merchant's own agent framework. A tenant admin creates them in the admin portal. Confidential clients get a secret, hashed at rest.
3. **DCR**, only where a client supports nothing else.

**Login and consent reuse the landing app.**
- `/oauth/authorize` validates the request.
- It stores a pending authorization (10 min TTL) and redirects to the landing login with `returnTo=/oauth/consent?req=<id>`. The landing app already propagates `returnTo` end to end.
- The consent page shows the client's name, logo, and redirect host; the tenant; the requested scopes in plain language; and an "unverified client" warning for DCR-registered clients.
- **Consent is refused, not shown, if the tenant lacks `enterprise_mcp`** or the user holds none of the permissions in any requested scope.
- Approving creates a **grant** `(user, tenant, client_id, scopes)` and a code, then redirects to the client.

### 4. Resource server (ai-layer `/mcp`)

`/mcp` moves out of `require_auth` and behind a dedicated `mcp_auth` layer. Each request goes through these steps:

1. **No or invalid token: `401`.** The response carries `WWW-Authenticate: Bearer resource_metadata="<OAUTH__RESOURCE origin>/.well-known/oauth-protected-resource/mcp", scope="logisticos.read"`.
2. **Verify the MCP access token.** Check ES256 against the JWKS (cached 10 min, refetched on an unknown `kid` at most once a minute), `iss == OAUTH__ISSUER`, `aud == OAUTH__RESOURCE`, and `exp`.
3. **Exchange the token** for a platform token at identity's internal `POST /v1/internal/oauth/exchange`. The request has RFC 8693's shape (`subject_token` = the MCP token). The route is authenticated the way identity's existing `/v1/internal/*` routes are, and the gateway refuses internal paths.
   - Identity re-reads the grant, the user, the user's roles, and the tenant's features.
   - It refuses if the grant is revoked, the user is disabled, or the tenant has lost `enterprise_mcp`.
   - Otherwise it mints an **HS256 platform token, at most 5 minutes**, carrying `permissions` = roles ∩ scopes and two new optional claims, `mcp_client_id` and `mcp_grant_id` (`#[serde(default)]`, so existing tokens still decode).
   - ai-layer caches the result per `jti` for at most 60 s. That bounds revocation lag to about a minute without putting identity on every call.
4. **R1's `mcp_tool_access(&claims, tool)` runs unchanged** against the exchanged claims. Internal-route tools still require `pid.is_none()`. `tenant_id` is still overwritten server-side.
5. **Tools forward the exchanged platform token downstream**, never the MCP token. R1's bearer forwarding already threads one bearer through `ToolContext`; this changes only which bearer it is.
6. **Audit.** `record_audit` adds `client_id` and `grant_id` to `AgentSession.trigger`, alongside the existing `user_id`, `email` and `client_ip`.

ai-layer also serves **Protected Resource Metadata** (RFC 9728) at `/.well-known/oauth-protected-resource/mcp` and, for clients that probe the root, at `/.well-known/oauth-protected-resource`:

```json
{ "resource": "<OAUTH__RESOURCE>",
  "authorization_servers": ["<OAUTH__ISSUER>"],
  "scopes_supported": ["logisticos.read", "logisticos.operate", "logisticos.billing"],
  "bearer_methods_supported": ["header"] }
```

### 5. Gateway

- **Public allowlist** (`is_public` in `services/api-gateway/src/bootstrap.rs`): `/.well-known/oauth-authorization-server`, `/.well-known/oauth-protected-resource` (and its `/mcp` suffix), `/.well-known/jwks.json`, `/oauth/authorize`, `/oauth/token`, `/oauth/revoke`, `/oauth/register`.
- **`/mcp` is upstream-authenticated.** The gateway verifies an MCP access token with the same `libs/auth` verifier ai-layer uses, only to key the rate limit by `tenant_id` + `client_id`, and forwards the request. ai-layer's `mcp_auth` stays authoritative.
- **Bridge tokens still pass.** While the bridge is on (§7), a platform token at `/mcp` is validated as today.
- `resolve_upstream`: the `/.well-known/oauth-protected-resource*` routes go to ai-layer; `/oauth/*`, `/.well-known/oauth-authorization-server` and `/.well-known/jwks.json` go to identity.
- `/oauth/token` and `/oauth/register` get their own per-IP rate limit, since they are unauthenticated writes.

### 6. Dynamic Client Registration is opt-in

Open DCR lets anyone on the internet create a client record. It is off by default (`OAUTH__DCR_ENABLED=false`). It is turned on only when a client the business needs supports neither Client ID Metadata Documents nor pre-registration. When it is on:

- Registration is rate-limited per IP.
- Redirect URIs must be https or loopback.
- The client is marked unverified on the consent screen.
- A client that never completes a grant expires after 30 days.

### 7. The static-token bridge stays, then closes

`MCP__ACCEPT_PLATFORM_TOKENS` (ai-layer and gateway) keeps today's behaviour. It defaults to `true` until OAuth is live for every Enterprise tenant, then flips to `false`. A platform token at `/mcp` logs `bridge=true` in the audit trigger, so the remaining bridge users are visible before the switch.

### 8. Grants are visible and revocable

- **Tenant admins** get a "Connected AI clients" view in the admin portal. It lists grants (client, user, scopes, created, last used) and can revoke one.
- **Users** can revoke their own grants from the same list.
- **Revoking a grant** revokes its refresh-token family. The next exchange refuses it, so the effect lands within the exchange cache's 60 s.
- **Grant lifecycle events** (created, refreshed, revoked, refused) are audit-logged with actor, tenant, IP and `client_id`.

## Alternatives considered

1. **A third-party authorization server** (Keycloak, Auth0, Okta).
   - **Rejected for now.** Roles, permissions, tenant features and tenancy live in identity. An external authorization server would either duplicate them (two sources of truth for who may do what) or call back into identity on every grant.
   - Keycloak adds a stateful service to operate. Hosted options add per-user cost and a data-residency question the platform's non-negotiables forbid answering casually.
   - Because ai-layer verifies by `iss` + JWKS, a later move to an external authorization server changes configuration, not the resource server.
2. **Add `aud` to HS256 platform tokens and have services check it.**
   - **Rejected.** It means changing validation in every service and redeploying all of them in lockstep.
   - Every legacy token lacks `aud`.
   - A symmetric key held by 20 services means any one compromised service can mint MCP tokens. A dedicated asymmetric key gives the same isolation with no change to any existing service.
3. **Opaque tokens plus introspection (RFC 7662).**
   - **Rejected.** It puts identity on every `/mcp` call.
   - Token exchange with a 60 s cache gives near-equal revocation latency at a fraction of the load. The exchange is needed anyway to stop passthrough.
4. **ai-layer mints the downstream platform token itself** (it already holds the HS256 secret).
   - **Rejected.** ai-layer would have to resolve roles to permissions and check grant revocation, both identity data. It would also mean a second service minting platform tokens.
5. **Keep the static-token bridge as the end state.**
   - **Rejected.** It offers no consent and no per-client revocation, and it requires long-lived secrets pasted into tools. It stays only as a migration path (§7).

## Consequences

- **Identity becomes a security-critical public surface:** the authorize, token, register and consent flows. The whole feature ships behind `OAUTH__ENABLED=false` and is enabled per environment only after a CISO review and a pen test of the authorization-server endpoints.
- **New secrets in Vault:** the ES256 signing key (`OAUTH__SIGNING_KEY_PEM`, `OAUTH__SIGNING_KID`) and the previous key during rotation. These are never in env files.
- **New identity tables** (schema-isolated, ADR-0012): `oauth_clients`, `oauth_pending_authorizations`, `oauth_grants`, `oauth_auth_codes` (hashed), `oauth_refresh_tokens` (hashed, `family_id`, `rotated_at`, `revoked_at`).
- **One added hop per `/mcp` call** on a cache miss (ai-layer → identity exchange). This is bounded by the 60 s cache and well inside the 200 ms P99 budget for a single internal call.
- **The R1 gate stays the single authority** for what a tool may do. Scopes only remove permissions. Downstream handlers that check permissions see the narrowed set; those that check only `require_auth` are protected by the R1 gate alone. That is the situation today, made no worse.
- **A hard prerequisite:** OAuth needs a stable public HTTPS origin for `OAUTH__ISSUER` and `OAUTH__RESOURCE`. While the platform domain is unreachable, OAuth cannot be enabled anywhere but a local or staging origin.
- **Multi-tenant users are out of scope.** A grant is bound to the tenant of the session that consented, as platform tokens are today.

## Open questions for the deciders

1. **Is DCR on at GA,** or only Client ID Metadata Documents plus pre-registration? The proposal is off.
2. **Are the access-token TTL (15 min) and exchange-cache TTL (60 s) right** for the revocation latency Enterprise contracts promise?
3. **Should headless merchant agents get `client_credentials`** bound to a tenant API key and a service-account user? This ADR leaves it out (Phase 4 of the plan), so there is no grant without a human consent.

## References

- MCP specification, *Authorization*: Protected Resource Metadata discovery, PKCE, resource indicators, no token passthrough.
- RFC 9728 (Protected Resource Metadata), RFC 8414 (Authorization Server Metadata), RFC 8707 (Resource Indicators), RFC 7636 (PKCE), RFC 8252 (native apps / loopback redirects), RFC 7591 (Dynamic Client Registration), RFC 7009 (revocation), RFC 8693 (token exchange), RFC 9700 (OAuth 2.0 Security BCP).
- Implementation plan: [2026-11-25-mcp-oauth-authorization.md](../superpowers/plans/2026-11-25-mcp-oauth-authorization.md)
