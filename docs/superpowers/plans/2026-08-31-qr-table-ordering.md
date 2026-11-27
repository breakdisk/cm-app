# QR Table Ordering — Standalone Restaurant

**Goal:** A diner scans a printed code on a table and can order, without an account.

**Architecture:** The QR encodes an identifier, never a session. Scanning it mints a short-lived, narrowly-scoped JWT bound to that table. Ordering then rides the existing basket/order model with zero courier legs.

**Tech Stack:** Rust, Axum, SQLx, `logisticos_auth`.

---

## Scope

ADR-0017 designs QR ordering for two shapes. Only one is buildable now.

| Shape | State |
|---|---|
| **Standalone restaurant** — one venue, one vendor, one leg | **This plan.** Unblocked. |
| Mall foodcourt — one table, many stalls, N legs | **Blocked.** Needs the acceptance barrier, which needs partial capture verified against a live NI sandbox. The code path exists (`59f92387`); NI's behaviour on a partial amount does not yet. |

The split is not cosmetic. A standalone venue has exactly one vendor, so an order from it has exactly one leg — which means it never needs a partial capture, and the whole money question the foodcourt raises simply does not arise. That is what makes it separable.

---

## The security posture this plan is mostly about

The token is printed on adhesive vinyl in a public room and is photographable from three metres by anyone walking past. **It must be worth nothing on its own.**

Four controls, in descending order of how much they actually bound the threat:

1. **A table has an open/closed state, gated on venue hours.** Ordering to table A-14 at 03:00 when the restaurant is shut must be impossible regardless of how valid the token is. This is not a security mechanism; it is an operational one, and it is the strongest control here.
2. **A cap on concurrent live sessions per table.** A four-top does not need fifty.
3. **Token rotation is an operator action.** "Reprint this table's code" is a button, so a leaked token is a five-minute fix rather than an incident.
4. **Rate limiting.** The weakest of the four, and the one that needs new machinery: `check_rate_limit` keys on `ratelimit:tenant:{tenant_id}` and sizes the window by subscription tier, so an unauthenticated request has neither. This endpoint sits outside the platform's rate-limiting model rather than being under-configured.

**Browser fingerprinting is rejected**, as in ADR-0017: it is behavioural tracking of an unconsented anonymous diner, against the platform's stated GDPR/PDPA position, and weak against a threat committed by someone holding a real phone at a real table.

---

## The anonymous principal

A new class of token, and the thing most deserving of review.

`Claims` gains `table_session: bool`, exactly mirroring the `onboarding` precedent: `#[serde(default)]` so every existing token still decodes, and a flag services can use as a belt-and-braces check alongside permission gating.

The minted token carries:

- a **synthetic `user_id`** — the session id. `Order.customer_id` stays required and non-null, so tracking, legs and the ledger need no changes at all.
- `email: ""` — there is no person to name.
- `permissions: []` — it can do nothing by permission. Every route it may reach must accept it explicitly.
- `table_session: true`.
- a **short expiry**, minutes not hours.

It is minted by omnideliv itself, which already holds a `JwtService`. It is not an identity-service user and never becomes one.

---

## File Structure

| File | Responsibility |
|---|---|
| `services/omnideliv/migrations/0027_venues_and_tables.sql` | Create: `venues`, `tables`, `table_sessions` |
| `services/omnideliv/src/domain/entities/venue.rs` | Create: `Venue`, `Table`, `TableSession`, and the hours/open rules |
| `services/omnideliv/src/domain/repositories/mod.rs` | Modify: `VenueRepository` |
| `services/omnideliv/src/infrastructure/db/venue_repo.rs` | Create: the queries |
| `services/omnideliv/src/api/http/tables.rs` | Create: the public scan route and the operator print sheet |
| `libs/auth/src/claims.rs` | Modify: `table_session` flag |

---

## Task 1: Schema

`venues` is separate from `vendors` because a venue is a *place with tables* and a vendor is a *business that sells*. A foodcourt is one venue with many vendors; a standalone restaurant is one venue with one. Collapsing them would make the foodcourt case unrepresentable later.

`table_token` is the printed secret: opaque, random, rotatable, and unique across the platform so a scan resolves without needing to know the venue first.

## Task 2: Domain — when is a table orderable

A pure function over venue hours, table status and `now`, so the rule is testable without a database and without a clock. Mirrors `leg_recovery::decide` and `recovery_service::decide`.

## Task 3: The scan endpoint

`POST /v1/omnideliv/tables/:table_token/session`, mounted **before** the auth layer — the same place `catalog::public_routes` and `health` sit, and for the same reason.

Refuses when: the token resolves to nothing, the table is closed, the venue is outside its hours, or the table already has its cap of live sessions. **A refusal must not say which** — a probing scanner learns nothing from a 404 it cannot distinguish from a closed table.

## Task 4: The print sheet

Operator-authenticated. Returns each table's label and its scan URL so a venue can print and laminate them, plus a rotate action that invalidates the old code.

---

## Definition of done

> **Run against a real database and a live service 2026-08-31.** All 27 migrations applied from empty; all four tables created. Verified: a valid code at an open table mints a token; an unknown token, a closed table and a venue narrowed to 03:00-04:00 are **all three indistinguishable 404s** while the log still distinguishes them; the session cap refuses the third scan at cap 2; the minted token carries `table_session: true`, no permissions, no roles, an empty email and a synthetic `user_id` equal to the session id; the print sheet needs auth (401 without) and returns scan URLs; **rotation kills the old printed code** (404) while the new one works (200); and a second tenant's operator cannot rotate this tenant's table (404).
>
> Venue local time read as Mon 10:09 against 02:09 UTC, which is what proves the UTC+8 offset is genuinely applied rather than the UTC wall clock.
>
> Unlike the previous subsystem's live run, this one surfaced **no defects** — the gates behaved as specified.

- [ ] Scanning a valid token at an open table returns a short-lived narrow token
- [ ] The same scan outside venue hours is refused, and refused indistinguishably from an unknown token
- [ ] A minted token carries no permissions and is marked `table_session`
- [ ] Rotating a table's token makes the printed one stop working
- [ ] The session cap per table is enforced
- [ ] `cargo test` and `cargo clippy --all-targets` clean

## Not in this plan

- **Ordering itself.** This lands the table, the identity and the gates. Basket-scoped-to-venue and the zero-courier-leg dine-in order are the next increment.
- **The foodcourt.** Blocked on the acceptance barrier, above.
- **Rate limiting machinery.** Named as a control, but it needs a limiter that works without a tenant, which is api-gateway work and does not belong in this diff. The session cap and the hours gate are what actually ship here.
