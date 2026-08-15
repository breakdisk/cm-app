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
RUSTSEC-2026-0253"

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
