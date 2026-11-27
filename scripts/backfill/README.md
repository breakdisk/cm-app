# Backfill scripts

One-off data fixes that a rollout depends on. Each is dry-run by default and
idempotent, and none runs as part of a deploy: a person runs them, reads the
report, and then applies.

## partner-memberships.sh

Creates `identity.partner_memberships` rows for partner users that existed before
the `pid` token claim. Required before `AUTH__ENFORCE_PARTNER_SCOPE=true`:
without a membership, a partner's next token has no `pid` and is refused.

**Run `partner-shipment-links.sh` first.** A `pid` takes effect at the user's next
login or refresh, not when scoping is enforced, and from then on the partner sees
only linked shipments. Memberships before links blank the partner's shipment list.
The full order is in [the rollout runbook](../../docs/runbooks/neighborhood-phase-0-rollout.md).

```bash
# production VPS: psql runs inside the postgres container
export PSQL="docker exec -i logisticos-postgres psql -U logisticos"

bash scripts/backfill/partner-memberships.sh            # dry-run: read the report
bash scripts/backfill/partner-memberships.sh --apply    # write
bash scripts/backfill/partner-memberships.sh --riders   # also bind drivers.carrier_id
```

Read the dry-run before applying:

| Line | Meaning | What to do |
|---|---|---|
| `unmatched_email` | A carrier whose `contact_email` matches no active `partner` user | Check the email is right. That carrier's partner cannot sign in with a bound token until fixed |
| `multiple_carriers` | One user matches two or more carriers. **Not inserted** | Decide which carrier the person acts for, then create that one by hand |
| `held_by_other_carrier` | The user already has an active membership for a different carrier. Left alone | Nothing, unless it is wrong |
| `skipped_invalid` | A row with a malformed id was dropped | Investigate: ids come from the database |

Riders are opt-in because binding them changes what a rider's token can read. Phase 0
only needs the partner portal users; store riders get a membership when their store
creates them.

Set `TENANT_ID` to restrict a run to one tenant. Run `test-partner-memberships.sh`
against a **disposable** container to check the script after changing it:

```bash
TEST_CONTAINER=my-disposable-postgres bash scripts/backfill/test-partner-memberships.sh
```

## partner-shipment-links.sh

Fills `order_intake.shipment_partner_links` for shipments a carrier handled before
the link consumer existed. A partner token is scoped to linked shipments, so
without this a partner would lose sight of its own history as soon as their
token first carries `pid` (their next login or refresh), whether or not scoping is
enforced yet. Run this **before** `partner-memberships.sh`; see
[the rollout runbook](../../docs/runbooks/neighborhood-phase-0-rollout.md). Sources: `carrier.sla_records` (allocations) and `driver_ops.tasks`
joined to drivers that belong to a carrier.

```bash
export PSQL="docker exec -i logisticos-postgres psql -U logisticos"
bash scripts/backfill/partner-shipment-links.sh          # dry-run: from_sla / from_tasks / unique / would_insert
bash scripts/backfill/partner-shipment-links.sh --apply
```

Run it **after** the new order-intake image is deployed: the consumer links new
work, this covers the past, and they overlap harmlessly (one row per
shipment and carrier). Test with `test-partner-shipment-links.sh` the same way.
