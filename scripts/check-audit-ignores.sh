#!/usr/bin/env bash
#
# Every advisory the Security Audit ignores must still deserve to be ignored.
#
# `cargo audit --ignore RUSTSEC-XXXX` is permanent by default. Once upstream
# ships a fix, or once a dependency change drags the crate into the build for
# real, the flag keeps suppressing it and nobody finds out — the job stays green
# for a reason that stopped being true. This script gives each ignore an expiry
# condition and fails when the condition no longer holds.
#
# Two checks per ignored advisory:
#
#   1. It must still be reported. If `cargo audit` no longer raises it, the
#      dependency was fixed or dropped, and the ignore is stale — delete it.
#      Without this, a resolved advisory keeps a permanent suppression on the
#      books and the next real occurrence of that ID is silently swallowed.
#
#   2. Where the justification is "this crate is never compiled", that has to be
#      true. `cargo tree -i <crate>` printing "nothing to print" is the proof;
#      the crate is present in Cargo.lock (which is why cargo audit sees it at
#      all) but reachable from no enabled feature.
#
# Note on reading `cargo tree -i`: it exits 0 *both* when a package is in the
# graph and when it is only in the lock — the difference is the "nothing to
# print" warning on stderr. Testing its exit code proves nothing. Match text.
#
set -uo pipefail

FAILURES=0

fail() { echo "FAIL: $*"; FAILURES=$((FAILURES + 1)); }
ok()   { echo "  ok   $*"; }

echo "Collecting current advisories..."
AUDIT_JSON="$(cargo audit --json 2>/dev/null || true)"

if [ -z "$AUDIT_JSON" ]; then
  echo "FAIL: cargo audit produced no JSON — cannot verify the ignore list."
  exit 1
fi

REPORTED="$(printf '%s' "$AUDIT_JSON" | python3 -c '
import json, sys
d = json.load(sys.stdin)
ids = set()
for v in d.get("vulnerabilities", {}).get("list", []) or []:
    ids.add(v["advisory"]["id"])
w = d.get("warnings", {}) or {}
# `warnings` is a map of kind -> list, e.g. {"unmaintained": [...], "unsound": [...]}
for entries in w.values():
    for e in entries or []:
        adv = e.get("advisory") or {}
        if adv.get("id"):
            ids.add(adv["id"])
print("\n".join(sorted(ids)))
')"

echo "Verifying each ignored advisory still earns its ignore..."
echo

# ── RUSTSEC-2023-0071 — rsa "Marvin Attack" timing sidechannel ───────────────
# Ignored because rsa is never compiled. It reaches Cargo.lock through
# sqlx-mysql, an optional dependency of sqlx that no feature of ours enables —
# we are Postgres-only. cargo audit reads Cargo.lock, which records optional
# dependencies whether or not any feature activates them, so it reports a crate
# that is in no binary we ship. There is no fixed version upstream, so the only
# alternatives are this or removing sqlx.
# Expiry: fails the moment rsa actually enters the build graph.
if ! printf '%s\n' "$REPORTED" | grep -qx "RUSTSEC-2023-0071"; then
  fail "RUSTSEC-2023-0071 is no longer reported — the ignore is stale, remove it."
else
  RSA_TREE="$(cargo tree -i rsa --all-features 2>&1 | grep -vE 'Downloading|Downloaded')"
  if printf '%s' "$RSA_TREE" | grep -q "nothing to print"; then
    ok "RUSTSEC-2023-0071 (rsa) — still reported, still not compiled."
  else
    fail "RUSTSEC-2023-0071 is ignored on the grounds that rsa is never compiled,
      but it is now IN THE BUILD GRAPH:

$(printf '%s' "$RSA_TREE" | head -6)

      Something enabled sqlx's mysql feature. Either turn it back off, or drop
      the ignore and treat the advisory as real — it is a real timing
      sidechannel once the code actually ships."
  fi
fi

# ── RUSTSEC-2026-0253 — lru: panic-safety use-after-free in LruCache::pop() ──
# Ignored with open eyes: this one IS compiled. lru comes from aws-sdk-s3's
# internals, no version of lru fixes it yet, and aws-sdk-s3 pins the major, so
# there is nothing to upgrade to. Unsoundness, not a remotely reachable
# vulnerability, and we never touch LruCache ourselves.
# Expiry: fails as soon as cargo audit stops reporting it, which is what a fixed
# lru or an aws-sdk-s3 that dropped it would look like.
if ! printf '%s\n' "$REPORTED" | grep -qx "RUSTSEC-2026-0253"; then
  fail "RUSTSEC-2026-0253 is no longer reported — a fix likely shipped.
      Remove the ignore from the Security Audit step in ci-rust.yml."
else
  ok "RUSTSEC-2026-0253 (lru) — still reported, still no fixed version."
fi

# ── RUSTSEC-2026-0258 — h2: unbounded empty DATA frames (HTTP/2 DoS) ────────
# The fix is h2 >= 0.4.16, and there is none in the 0.3 line at all. The public
# edge runs axum 0.7 -> hyper 1 -> h2 0.4 and IS fixed: the lockfile carries
# 0.4.18. What remains is h2 0.3.27, and the only path to it is
# tonic 0.11 -> axum 0.6 -> hyper 0.14 — the internal gRPC transport, which sits
# behind Istio mTLS and takes no traffic from the internet.
#
# Two expiry conditions, because "no fix exists" is not the whole justification
# here — "it is not on the public edge" does half the work:
#   1. Still reported. A tonic upgrade would drop h2 0.3 entirely.
#   2. hyper 0.14 is still reachable ONLY through tonic. The day anything else
#      pulls it — a service pinning axum 0.6, say — an unpatched HTTP/2 stack is
#      serving something, and this ignore stops being honest.
if ! printf '%s\n' "$REPORTED" | grep -qx "RUSTSEC-2026-0258"; then
  fail "RUSTSEC-2026-0258 is no longer reported — tonic likely moved off h2 0.3.
      Remove the ignore from ci-rust.yml, deny.toml and this script."
else
  # Only the first two levels matter: who depends on hyper 0.14 directly, and
  # who depends on them. The justification is that every such path passes
  # through tonic. Anything deeper is downstream *of* tonic — the OTLP exporter,
  # for instance — which is a gRPC client, not another HTTP/2 server.
  #
  # An earlier version of this check compared every crate name in the whole
  # reverse tree and failed on opentelemetry-otlp, which is exactly that case.
  H2_TREE="$(cargo tree -i hyper@0.14 --all-features --depth 2 2>&1 | grep -vE 'Downloading|Downloaded')"

  # Crate names at those two levels, hyper itself excluded. Expected: axum (0.6),
  # hyper-timeout (a tonic-only helper), and tonic.
  NEAR="$(echo "$H2_TREE" | grep -oE '[a-z0-9_-]+ v[0-9]+' | awk '{print $1}' | grep -vx hyper | sort -u)"
  UNEXPECTED="$(echo "$NEAR" | grep -vxE 'axum|hyper-timeout|tonic' || true)"

  if echo "$H2_TREE" | grep -q 'tonic v0' && [ -z "$UNEXPECTED" ]; then
    ok "RUSTSEC-2026-0258 (h2 0.3) — still reported, still gRPC-only, still no 0.3 fix."
  else
    fail "RUSTSEC-2026-0258 is ignored on the grounds that unpatched h2 0.3 is
      reachable only through tonic's gRPC transport. That no longer holds:

$(echo "$H2_TREE" | head -8)

      Unexpected direct dependents: ${UNEXPECTED:-<tonic missing>}

      An unpatched HTTP/2 stack may now be serving traffic. Either put it back
      behind tonic, upgrade tonic to a hyper-1.x release, or drop the ignore and
      treat the DoS as real."
  fi
fi

# ── The list lives in three places and they must agree ──────────────────────
# cargo-audit takes --ignore flags in ci-rust.yml; cargo-deny reads deny.toml
# and knows nothing about those flags; this script hard-codes the justifications.
# Suppressing a finding in one place and not the others means the audit job
# fails at a different step with the same advisory — or worse, an advisory is
# silently suppressed in deny.toml with no justification recorded anywhere.
echo
echo "Checking the ignore list agrees across all three places..."

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# What this script vouches for — keep in sync with the blocks above.
VOUCHED="RUSTSEC-2023-0071
RUSTSEC-2026-0253
RUSTSEC-2026-0258"

IN_WORKFLOW="$(grep -oE '\-\-ignore RUSTSEC-[0-9]{4}-[0-9]{4}' "$ROOT/.github/workflows/ci-rust.yml" \
  | sed 's/--ignore //' | sort -u)"

IN_DENY="$(sed -n '/^\[advisories\]/,/^\[/p' "$ROOT/deny.toml" \
  | grep -oE '"RUSTSEC-[0-9]{4}-[0-9]{4}"' | tr -d '"' | sort -u)"

VOUCHED_SORTED="$(printf '%s\n' "$VOUCHED" | sort -u)"

if [ "$IN_WORKFLOW" != "$VOUCHED_SORTED" ]; then
  fail "ci-rust.yml's --ignore flags do not match what this script vouches for.
      ci-rust.yml : $(echo "$IN_WORKFLOW" | tr '\n' ' ')
      vouched here: $(echo "$VOUCHED_SORTED" | tr '\n' ' ')"
else
  ok "ci-rust.yml --ignore flags match."
fi

if [ "$IN_DENY" != "$VOUCHED_SORTED" ]; then
  fail "deny.toml's [advisories] ignore list does not match what this script vouches for.
      deny.toml   : $(echo "$IN_DENY" | tr '\n' ' ')
      vouched here: $(echo "$VOUCHED_SORTED" | tr '\n' ' ')"
else
  ok "deny.toml ignore list matches."
fi

echo
if [ "$FAILURES" -gt 0 ]; then
  echo "$FAILURES problem(s) with the advisory ignore list."
  exit 1
fi

echo "All ignored advisories still justify their ignore, in all three places."
