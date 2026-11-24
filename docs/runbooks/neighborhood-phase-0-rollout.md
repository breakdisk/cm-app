# Neighborhood network Phase 0: rollout runbook

Phase 0 binds partner tokens to one partner (`pid`), scopes partner reads to that
partner's rows, and lets a finished rider receive new work. It is rolled out behind
three flags, all default `false` and all set explicitly in `docker-compose.yml`:

| Flag | Services | Effect |
|---|---|---|
| `AUTH__ENFORCE_PARTNER_SCOPE` | order-intake, dispatch, driver-ops, hub-ops, payments, analytics, delivery-experience | Refuse a partner-role token that has no `pid` (403 `PARTNER_CONTEXT_REQUIRED`). Read **once per process**. |
| `DISPATCH__CLOSE_ASSIGNMENTS` | dispatch | Write `completed` assignments when a rider's work finishes. |
| `MAPBOX__PERMANENT_GEOCODING` | order-intake | Send `permanent=true` to Mapbox. Only after the account is confirmed for it. |

Each service logs its effective value once at startup (`partner_scope_enforced`,
`close_assignments`, `permanent_geocoding`). Check them after every deploy.

Every step needs the owner's go-ahead. Do them in order and do not skip ahead.

## Steps

1. **Deploy all images with all three flags `false`.** Nothing changes for users.

2. **Run the links backfill first.** `scripts/backfill/partner-shipment-links.sh`
   as a dry-run, review the counts with the owner, then `--apply`. Check the
   inserted row count against the dry-run's `would_insert`.

3. **Only then run `partner-memberships.sh`** (dry-run, review, `--apply`). The order
   is not cosmetic: a `pid` takes effect at the user's **next login or refresh**, not
   at enforcement. From that moment the partner sees only linked shipments, so
   memberships before links blank their shipment list.

4. **Do not run `--riders` until one rider has been smoke-tested end to end**
   (sign in, receive a job, finish it, receive another). Binding riders changes
   what their tokens can read. A user whose memberships are all suspended is
   refused a token (403 `PARTNER_MEMBERSHIP_INACTIVE`) at their next login or
   refresh; they do not fall back to a tenant-wide driver token.

5. **Wait one full token lifetime**, then confirm zero `partner_scope_missing_pid`
   log lines across all seven services for that period. Any line is a partner
   session that would be refused after step 6.

6. **Set `AUTH__ENFORCE_PARTNER_SCOPE=true` on all seven services and restart them
   together.** The value is read once per process, so a partial flip leaves some
   services enforcing and others not. Confirm from the startup logs that every
   service reports `partner_scope_enforced=true`.

7. **Set `DISPATCH__CLOSE_ASSIGNMENTS=true`, only after every dispatch replica runs
   an image at or after `83045370`.** An older image reads a `completed` assignment
   as `Pending`. **Rollback below `83045370` requires turning the flag off first**,
   then letting the sweep stop, then rolling the image back.

8. **A partner link the consumer gave up on** (4 attempts) is not retried again. It
   is repaired by re-running `partner-shipment-links.sh --apply`, which is
   idempotent.

9. **Verify** with the plan's Task 0.17 Steps 4 to 6
   (`docs/superpowers/plans/2026-11-23-neighborhood-network-phase-0-partner-safety.md`):
   an existing partner login still loads Dashboard, Orders, Drivers, Manifests, SLA
   and Payouts; a real rider finishes a job and receives another; a partner token
   cannot read another partner's shipments, wallet, manifest or riders;
   `cargomarket.net` is reachable; PIN delivery works through Twilio.

## Rolling back enforcement

Set `AUTH__ENFORCE_PARTNER_SCOPE=false` on all seven services and restart them
together. Tokens that already carry `pid` stay scoped; that is intended.
