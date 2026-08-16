# OmniDeliv Deployment & Seed Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the two new services reachable, the two new frontends buildable, and the hero flow runnable — none of which any plan in the set currently covers.

**Architecture:** ADR-0009's subdomain-per-product topology, which turns out to need real work first: `resolve_upstream` is hardcoded in the gateway binary, so "the same Rust binary with different routing config" is not true today. This plan makes it true, then stands up `omnideliv.api.cargomarket.net`, adds compose entries, and seeds enough data to execute the hero flow end to end.

---

## Why this plan exists

No plan in the set mentions a gateway, Dokploy, docker-compose or an env var. Plans 1–10 produce two new services and two new frontends that nothing can reach. Spec §10 lists the gateway as a **blocking** prerequisite.

Tracing the routing also found two collisions that would have surfaced at runtime:

| Path | Existing owner | Would collide with |
|---|---|---|
| `/v1/orders` | `order_intake_url` (LogisticOS shipments) | OmniDeliv `/v1/orders/checkout` |
| `/v1/assignments` | `dispatch_url` (LogisticOS gig offers) | field-ops `/v1/assignments/offer` |

ADR-0009 chose subdomains precisely to avoid this. But the choice is only real if a second gateway can exist, and `services/api-gateway/src/proxy/mod.rs` hardcodes its routing table in an `if/else` chain — so today a second gateway means a second binary. Task 1 fixes that.

---

## Dependencies

**Requires Plans 2, 3 and 6** at minimum (a service and a frontend to deploy). Best run after 9 and 10, so what gets deployed actually works.

---

## Task 1: Config-driven gateway routing

**Files:**
- Modify: `services/api-gateway/src/proxy/mod.rs`, `src/config.rs`

- [ ] **Step 1: Write the failing test**

```rust
// services/api-gateway/src/proxy/mod.rs — tests block
#[cfg(test)]
mod routing_tests {
    use super::*;

    fn table() -> RouteTable {
        RouteTable::from_rules(vec![
            RouteRule { prefix: "/v1/omnideliv".into(),        upstream: "http://omnideliv:8091".into() },
            RouteRule { prefix: "/v1/omnideliv/orders".into(), upstream: "http://orders:2".into() },
            RouteRule { prefix: "/v1/o".into(),                upstream: "http://short:1".into() },
        ])
    }

    #[test]
    fn the_longest_matching_prefix_wins() {
        // "/v1/o" and "/v1/omnideliv" also match, but "/v1/omnideliv/orders"
        // is more specific. First-match on declaration order would make routing
        // depend on config ordering, which is exactly the fragility this
        // replaces — and is the bug the existing if/else chain already has.
        assert_eq!(table().resolve("/v1/omnideliv/orders/checkout"), Some("http://orders:2"));
    }

    #[test]
    fn an_unmatched_path_resolves_to_nothing() {
        assert_eq!(table().resolve("/v1/unknown"), None);
    }

    /// Internal routes must never be reachable from a public gateway, whatever
    /// the config says — defence in depth over each service's own guard.
    #[test]
    fn internal_paths_are_refused_regardless_of_config() {
        let t = RouteTable::from_rules(vec![RouteRule {
            prefix: "/v1/internal".into(), upstream: "http://identity:8001".into(),
        }]);
        assert_eq!(t.resolve("/v1/internal/token-exchange"), None);
        assert_eq!(t.resolve("/v1/foo/internal/bar"), None);
    }

    #[test]
    fn a_prefix_match_must_be_on_a_segment_boundary() {
        // "/v1/ordersomething" is not an orders path.
        assert_eq!(table().resolve("/v1/ordersomething"), None);
    }
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `CARGO_INCREMENTAL=0 cargo test -p logisticos-api-gateway routing_tests`
Expected: FAIL to compile — `cannot find type 'RouteTable'`.

- [ ] **Step 3: Implement**

```rust
/// One routing rule, loaded from config.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct RouteRule {
    pub prefix:   String,
    pub upstream: String,
}

/// Path-prefix routing table.
///
/// Replaces the hardcoded if/else chain so one binary can serve any product's
/// gateway with different config — which is what ADR-0009 assumes when it says
/// every gateway is "the same Rust binary with different routing config".
#[derive(Debug, Clone)]
pub struct RouteTable {
    /// Sorted longest-prefix-first, so resolution does not depend on the order
    /// rules happen to appear in the config file.
    rules: Vec<RouteRule>,
}

impl RouteTable {
    pub fn from_rules(mut rules: Vec<RouteRule>) -> Self {
        rules.sort_by_key(|r| std::cmp::Reverse(r.prefix.len()));
        Self { rules }
    }

    pub fn resolve(&self, path: &str) -> Option<&str> {
        // Unconditional, before any rule is consulted: an internal route must
        // never be publicly reachable even if someone adds a rule for it.
        if path.starts_with("/v1/internal") || path.contains("/internal/") {
            return None;
        }

        self.rules
            .iter()
            .find(|r| {
                path.strip_prefix(r.prefix.as_str())
                    // A prefix match must end on a segment boundary, or
                    // "/v1/ordersomething" would route as an orders path.
                    .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
            })
            .map(|r| r.upstream.as_str())
    }
}
```

Load `routes` from config as `Vec<RouteRule>`, keeping the current hardcoded chain as the default so the existing LogisticOS gateway keeps working with no config change.

- [ ] **Step 4: Run the tests**

Run: `CARGO_INCREMENTAL=0 cargo test -p logisticos-api-gateway`
Expected: PASS, including the gateway's existing tests.

- [ ] **Step 5: Commit**

```bash
git add services/api-gateway/
git commit -m "feat(api-gateway): config-driven routing table

ADR-0009 assumes every gateway is the same binary with different routing
config; resolve_upstream hardcoded its table, so that was not true. Rules sort
longest-prefix-first so resolution does not depend on config ordering, and
prefix matches must land on a segment boundary."
```

---

## Task 2: The OmniDeliv gateway

**Files:**
- Create: `infra/gateways/omnideliv.routes.toml`, `infra/gateways/logistics.routes.toml`

- [ ] **Step 1: Write the route configs**

```toml
# infra/gateways/omnideliv.routes.toml
# omnideliv.api.cargomarket.net
#
# The subdomain split means /v1/orders *here* could safely mean OmniDeliv
# orders. The services nonetheless serve prefixed paths, decided before these
# routes were ever called, for two reasons the host split does not cover:
#
#   1. This gateway does not exist yet. Until it does there is one gateway, and
#      an unprefixed POST /v1/orders resolves to order-intake, where it does
#      not 404 — it succeeds and creates a real shipment.
#   2. field-ops is a *platform* tier reachable from both gateways. Unprefixed,
#      /v1/assignments would mean field-ops here and dispatch on the logistics
#      host — the same path naming two services depending on hostname. The
#      driver app already calls PUT /v1/assignments/:id/accept against
#      dispatch; a shared or merged courier app would make that ambiguity a bug.
#
# The /v1/omnideliv prefix is redundant once this gateway is live and may be
# dropped then. The /v1/field-ops prefix is permanent — it is what makes the
# tier addressable identically from every product.

[[routes]]
prefix   = "/v1/omnideliv"
upstream = "http://omnideliv:8091"

# Platform tier, reachable from this gateway because the app talks to one host.
[[routes]]
prefix   = "/v1/auth"
upstream = "http://identity:8001"

[[routes]]
prefix   = "/v1/field-ops"
upstream = "http://field-ops:8090"
```

```toml
# infra/gateways/logistics.routes.toml
# logistics.api.cargomarket.net — the existing table, made explicit.
# Extracting it from the binary is what lets a second gateway exist; the
# contents are unchanged, so the running gateway behaves identically.
# ... (transcribe every arm of the current resolve_upstream chain)
```

- [ ] **Step 2: Verify the existing gateway is unchanged**

The risk in this task is silently altering LogisticOS routing. Diff the extracted table against the code it replaces:

```bash
rg -o 'path\.starts_with\("(/v1/[a-z-]+)"\)' -r '$1' services/api-gateway/src/proxy/mod.rs | sort -u > /tmp/before.txt
rg -o '^prefix\s*=\s*"(/v1/[a-z-]+)"' -r '$1' infra/gateways/logistics.routes.toml | sort -u > /tmp/after.txt
diff /tmp/before.txt /tmp/after.txt && echo "ROUTES MATCH"
```

Expected: `ROUTES MATCH`. Any difference is a route about to be dropped or added — resolve it before deploying.

- [ ] **Step 3: Commit**

```bash
git add infra/gateways/
git commit -m "feat(infra): per-product gateway routing tables

OmniDeliv gets its own subdomain, so /v1/orders there is an OmniDeliv order
and does not collide with LogisticOS shipments. The logistics table is a
verbatim extraction of the current chain, diffed against it to prove nothing
moved."
```

---

## Task 3: Compose and environment

**Files:**
- Modify: `docker-compose.yml`, `.env.example`
- Modify: `.github/workflows/build-images.yml`

- [ ] **Step 1: Add the services**

Following the existing entry shape (see `pod` at line ~758):

```yaml
  field-ops:
    image: ghcr.io/breakdisk/logisticos-service-field-ops:latest
    container_name: logisticos-field-ops
    restart: unless-stopped
    ports:
      - 8090:8090
    environment:
      APP__HOST: "0.0.0.0"
      APP__PORT: "8090"
      APP__ENV: development
      DATABASE__URL: postgres://logisticos:password@postgres:5432/svc_field_ops
      DATABASE__MAX_CONNECTIONS: "5"
      KAFKA__BROKERS: kafka:29092
      KAFKA__GROUP_ID: field-ops-dev
      AUTH__JWT_SECRET: ${JWT_SECRET:-dev-jwt-secret-REPLACE-WITH-32CHAR-RANDOM-VALUE-123}
      CLAIM_TTL_SECS: "120"
    depends_on: [postgres, kafka]
    healthcheck:
      # /health is deliberately unauthenticated — an authenticated probe
      # returns 401, curl -sf fails, and the service shows red while being
      # perfectly healthy. That exact mistake has 8 services red for 11 days.
      test: ["CMD", "curl", "-sf", "http://localhost:8090/health"]
      interval: 30s
      timeout: 5s
      retries: 3

  omnideliv:
    image: ghcr.io/breakdisk/logisticos-service-omnideliv:latest
    container_name: logisticos-omnideliv
    restart: unless-stopped
    ports:
      - 8091:8091
    environment:
      APP__HOST: "0.0.0.0"
      APP__PORT: "8091"
      APP__ENV: development
      DATABASE__URL: postgres://logisticos:password@postgres:5432/svc_omnideliv
      DATABASE__MAX_CONNECTIONS: "5"
      KAFKA__BROKERS: kafka:29092
      KAFKA__GROUP_ID: omnideliv-dev
      AUTH__JWT_SECRET: ${JWT_SECRET:-dev-jwt-secret-REPLACE-WITH-32CHAR-RANDOM-VALUE-123}
      CLAUDE_API_KEY: ${CLAUDE_API_KEY:-}
      CLAUDE_MODEL: claude-opus-4-6
      CLAUDE_MAX_TOKENS: "8192"
      STOCK_FRESHNESS_MINS: "30"
      SERVICES__FIELD_OPS_URL: http://field-ops:8090
    depends_on: [postgres, kafka, field-ops]
    healthcheck:
      test: ["CMD", "curl", "-sf", "http://localhost:8091/health"]
      interval: 30s
      timeout: 5s
      retries: 3

  omnideliv-gateway:
    image: ghcr.io/breakdisk/logisticos-service-api-gateway:latest
    container_name: logisticos-omnideliv-gateway
    restart: unless-stopped
    ports:
      - 8100:8100
    environment:
      APP__PORT: "8100"
      GATEWAY__ROUTES_FILE: /etc/gateway/routes.toml
      AUTH__JWT_SECRET: ${JWT_SECRET:-dev-jwt-secret-REPLACE-WITH-32CHAR-RANDOM-VALUE-123}
    volumes:
      - ./infra/gateways/omnideliv.routes.toml:/etc/gateway/routes.toml:ro
    depends_on: [omnideliv, field-ops, identity]
```

> **`CLAUDE_API_KEY` defaults to empty on purpose.** A developer with no key gets a working stack where the manual order path (Plan 8) functions and the mesh returns an error — which is exactly the degraded behaviour the design promises, exercised by default rather than only in tests.

- [ ] **Step 2: Create the databases**

`svc_field_ops` and `svc_omnideliv` must exist before either service starts. Add both to whatever provisions the others (`scripts/db/` or the postgres init script), following the existing pattern.

- [ ] **Step 3: Add the Kafka topics**

In `scripts/create-kafka-topics.sh`, add `fieldops.courier`, `omnideliv.orders`, `omnideliv.baskets`.

- [ ] **Step 4: Add the images to CI**

Add `field-ops` and `omnideliv` to the service matrix in `build-images.yml`, and `vendor-console` and `omnideliv-app` to `ci-frontend.yml` if Plans 6 and 7 have not already.

- [ ] **Step 5: Verify the stack comes up**

```bash
docker compose up -d field-ops omnideliv omnideliv-gateway
sleep 20
curl -sf localhost:8090/health && echo " field-ops OK"
curl -sf localhost:8091/health && echo " omnideliv OK"
docker compose ps field-ops omnideliv --format '{{.Name}} {{.Status}}'
```

Expected: both `/health` return 200 **without a token**, and both containers report `healthy`. A container stuck at `starting` with a passing curl means the healthcheck command is wrong; one reporting `unhealthy` with a passing curl usually means the probe is hitting an authenticated path.

- [ ] **Step 6: Commit**

```bash
git add docker-compose.yml scripts/ .github/workflows/
git commit -m "feat(infra): compose entries, databases and topics for the new services

CLAUDE_API_KEY defaults to empty so a developer with no key gets a stack where
the manual order path works and the mesh degrades — the promised behaviour,
exercised by default rather than only in tests."
```

---

## Task 4: Seed the hero flow

Nothing in the set creates a tenant, a vendor, catalog items or a courier — so the flow every plan is built around cannot actually be run.

**Files:**
- Create: `scripts/seed-omnideliv.sh`

- [ ] **Step 1: Write the seed**

```bash
#!/usr/bin/env bash
# Seeds the hero flow: "Dinner for two from Kuya's, and we're out of milk and eggs."
#
# Idempotent — safe to re-run. Uses fixed UUIDs so a developer can reference the
# same vendor across restarts, and so a failed run can simply be repeated.
set -euo pipefail

DB="${DB:-svc_omnideliv}"
FIELD_OPS_DB="${FIELD_OPS_DB:-svc_field_ops}"
PSQL="docker exec -i logisticos-postgres psql -U logisticos -v ON_ERROR_STOP=1"

TENANT="00000000-0000-0000-0000-000000000001"   # the existing dev tenant
KUYAS="11111111-0000-0000-0000-000000000001"
PUREGOLD="11111111-0000-0000-0000-000000000002"

echo "Seeding vendors…"
$PSQL -d "$DB" <<SQL
INSERT INTO omnideliv.vendors (id, tenant_id, vertical, name, address, lat, lng, prep_time_minutes, commission_bps, status)
VALUES
  ('$KUYAS',    '$TENANT', 'restaurant', 'Kuya''s Silog House', '12 Mabini St, Manila', 14.5995, 120.9842, 20, 1500, 'active'),
  ('$PUREGOLD', '$TENANT', 'grocery',    'Puregold Ermita',     '8 Padre Faura, Manila', 14.5820, 120.9830,  5, 1200, 'active')
ON CONFLICT (id) DO UPDATE SET status = 'active';
SQL

echo "Seeding catalog…"
$PSQL -d "$DB" <<SQL
INSERT INTO omnideliv.catalog_items (id, tenant_id, vendor_id, sku, name, price_cents, allergens, dietary_tags)
VALUES
  ('22222222-0000-0000-0000-000000000001', '$TENANT', '$KUYAS',    'tapsilog',  'Tapsilog',        17000, '{}',      '{}'),
  ('22222222-0000-0000-0000-000000000002', '$TENANT', '$KUYAS',    'bangsilog', 'Bangsilog',       16000, '{fish}',  '{}'),
  ('22222222-0000-0000-0000-000000000003', '$TENANT', '$PUREGOLD', 'milk-1l',   'Fresh Milk 1L',    8500, '{dairy}', '{}'),
  ('22222222-0000-0000-0000-000000000004', '$TENANT', '$PUREGOLD', 'eggs-12',   'Eggs (dozen)',    12000, '{eggs}',  '{}'),
  ('22222222-0000-0000-0000-000000000005', '$TENANT', '$PUREGOLD', 'eggs-12-b', 'Eggs, Farm Fresh',10800, '{eggs}',  '{}')
ON CONFLICT (id) DO NOTHING;

-- Availability is inserted by save_item in production. Seeded here explicitly
-- so the freshness clock starts now rather than at whatever the default was.
INSERT INTO omnideliv.item_availability (item_id, tenant_id, state, updated_at)
SELECT id, tenant_id, 'available', NOW() FROM omnideliv.catalog_items WHERE tenant_id = '$TENANT'
ON CONFLICT (item_id) DO UPDATE SET state = 'available', updated_at = NOW();

-- One item out of stock, so the substitution path has something to do. Without
-- this the hero flow never exercises Screen C, which is half the design.
UPDATE omnideliv.item_availability
   SET state = 'out_of_stock', updated_at = NOW()
 WHERE item_id = '22222222-0000-0000-0000-000000000004';
SQL

echo "Seeding a courier…"
$PSQL -d "$FIELD_OPS_DB" <<SQL
INSERT INTO field_ops.couriers (id, tenant_id, user_id, first_name, last_name, phone, status, last_lat, last_lng, last_seen_at)
VALUES ('33333333-0000-0000-0000-000000000001', '$TENANT', gen_random_uuid(),
        'Rico', 'M', '+639170000001', 'available', 14.5900, 120.9800, NOW())
ON CONFLICT (id) DO UPDATE SET status = 'available', last_seen_at = NOW();
SQL

echo
echo "Seeded. Try:"
echo "  Kuya's Silog House : $KUYAS"
echo "  Puregold Ermita    : $PUREGOLD"
echo "  Eggs (dozen) is OUT OF STOCK — the substitution path has something to propose."
```

- [ ] **Step 2: Run it**

```bash
chmod +x scripts/seed-omnideliv.sh && ./scripts/seed-omnideliv.sh && ./scripts/seed-omnideliv.sh
```

Expected: succeeds twice. Re-running must be safe — a seed that only works on an empty database is a seed nobody uses twice.

- [ ] **Step 3: Commit**

```bash
git add scripts/seed-omnideliv.sh
git commit -m "feat(scripts): seed the OmniDeliv hero flow

Fixed UUIDs so a developer can reference the same vendor across restarts, and
idempotent so a failed run is simply repeated. One item is seeded out of stock
— without that the hero flow never exercises the substitution screen, which is
half the design."
```

---

## Task 5: Hero-flow smoke test

**Files:**
- Create: `scripts/omnideliv-smoke.sh`

- [ ] **Step 1: Write the smoke test**

```bash
#!/usr/bin/env bash
# End-to-end smoke test through the gateway, exercising the manual order path.
#
# Deliberately the manual path, not the mesh: it must pass with no Claude
# credentials, so CI can run it on every push without an API key or spend.
set -euo pipefail

GW="${GW:-http://localhost:8100}"
TOKEN="${TOKEN:?set TOKEN to a customer JWT for the dev tenant}"
KUYAS="11111111-0000-0000-0000-000000000001"
TAPSILOG="22222222-0000-0000-0000-000000000001"

api() {
  curl -sf -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" "$@"
}

echo "1/5  health"
curl -sf "$GW/../health" >/dev/null 2>&1 || true
curl -sf http://localhost:8091/health >/dev/null && echo "     omnideliv healthy"

echo "2/5  browse"
api "$GW/v1/omnideliv/vendors?vertical=restaurant&lat=14.5995&lng=120.9842" | grep -q "$KUYAS" \
  && echo "     Kuya's is listed"

echo "3/5  create basket"
BASKET=$(api -X POST "$GW/v1/omnideliv/baskets" -d '{}' | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')
[ -n "$BASKET" ] && echo "     basket $BASKET"

echo "4/5  add a line"
api -X POST "$GW/v1/omnideliv/baskets/$BASKET/lines" \
    -d "{\"vendor_id\":\"$KUYAS\",\"item_id\":\"$TAPSILOG\",\"qty\":2}" >/dev/null
TOTAL=$(api "$GW/v1/omnideliv/baskets/$BASKET" | sed -n 's/.*"goods_total_cents":\([0-9]*\).*/\1/p')
[ "$TOTAL" = "34000" ] || { echo "     expected 34000, got $TOTAL"; exit 1; }
echo "     total $TOTAL"

echo "5/5  checkout"
ORDER=$(api -X POST "$GW/v1/omnideliv/orders/checkout" \
  -d "{\"basket_id\":\"$BASKET\",\"tip_cents\":0,\"delivery_lat\":14.5995,\"delivery_lng\":120.9842}" \
  | sed -n 's/.*"order_id":"\([^"]*\)".*/\1/p')
[ -n "$ORDER" ] || { echo "     checkout produced no order"; exit 1; }
echo "     order $ORDER"

echo
echo "PASS — an order was placed with no LLM in the path."
```

- [ ] **Step 2: Run it**

```bash
chmod +x scripts/omnideliv-smoke.sh
TOKEN="<a dev customer JWT>" ./scripts/omnideliv-smoke.sh
```

Expected: `PASS`. A failure at step 5 with `503` means no courier is available — re-run the seed.

- [ ] **Step 3: Commit**

```bash
git add scripts/omnideliv-smoke.sh
git commit -m "feat(scripts): hero-flow smoke test through the gateway

Exercises the manual path deliberately, so it passes with no Claude
credentials and CI can run it on every push without an API key or spend."
```

---

## Definition of done

- [ ] `cargo test -p logisticos-api-gateway` — routing tests pass, existing tests unchanged
- [ ] The extracted logistics route table diffs clean against the current chain
- [ ] `docker compose up -d field-ops omnideliv omnideliv-gateway` — all three reach `healthy`
- [ ] `curl -sf localhost:8090/health` and `:8091/health` return 200 **without a token**
- [ ] `./scripts/seed-omnideliv.sh` succeeds twice in a row
- [ ] `./scripts/omnideliv-smoke.sh` passes **with `CLAUDE_API_KEY` unset**

## Deployment notes

- **Dokploy:** each gateway is its own app with its own Traefik host rule, per ADR-0009. `omnideliv-gateway` at `omnideliv.api.cargomarket.net`; the services themselves are not publicly exposed.
- **Migrations before images.** A migration that cannot apply pins a service to its last-good image and the failure is silent — that is how `engagement` sat seven weeks behind. Check `_sqlx_migrations` in `svc_omnideliv` and `svc_field_ops` after the first deploy rather than assuming.
- **`Cargo.lock` changes rebuild everything.** Adding two workspace members means `build-images.yml` rebuilds all 20+ services, not just the new two. Expect a long first pipeline.
