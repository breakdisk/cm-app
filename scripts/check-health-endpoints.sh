#!/usr/bin/env bash
# /health must not sit behind the auth layer.
#
# A liveness probe cannot present a JWT. If `require_auth` is layered over the
# same router that defines `/health`, the healthcheck answers 401 forever and
# the container reports unhealthy while serving traffic perfectly well.
#
# On 2026-08-09 that was true of **nine** services at once — ai-layer, analytics,
# business-logic, carrier, cdp, engagement, delivery-experience, fleet, and
# marketing (which returned 404, having no health route at all). They had been
# reporting unhealthy for weeks, which meant "unhealthy" carried no information:
# a real outage would have looked exactly the same.
#
# The check that was here before could not have caught it. It grepped
# `src/main.rs` and `src/router.rs` — files these services do not use for
# routing — and printed a WARNING with no exit code. This one fails.
#
# Two rules, both static, no cluster needed:
#   1. every service defines /health somewhere
#   2. the function that defines it is NOT the function `require_auth` wraps
set -euo pipefail

cd "$(dirname "$0")/.."

fail=0

for dir in services/*/; do
  svc=$(basename "$dir")
  [ -d "$dir/src" ] || continue
  # Library crates with no bootstrap are not deployed services.
  [ -f "$dir/src/bootstrap.rs" ] || continue

  if ! grep -rq '"/health"' "$dir/src" 2>/dev/null; then
    echo "MISSING  $svc defines no /health route"
    fail=$((fail + 1))
    continue
  fi

  # Which function defines /health, and which one does require_auth wrap?
  verdict=$(python3 - "$dir" <<'PY'
import re, sys, pathlib

root = pathlib.Path(sys.argv[1], "src")

def fn_defining(pattern, files):
    """Name of the fn whose body contains `pattern`."""
    hits = set()
    for f in files:
        src = f.read_text(encoding="utf-8", errors="ignore")
        # Split on top-level `fn name(` boundaries and keep the owning name.
        for m in re.finditer(r"\bfn\s+(\w+)\s*\(", src):
            start = m.end()
            depth, i = 0, start
            # Walk to the opening brace of the body, then to its close.
            while i < len(src) and src[i] != "{":
                i += 1
            body_start = i
            while i < len(src):
                if src[i] == "{":
                    depth += 1
                elif src[i] == "}":
                    depth -= 1
                    if depth == 0:
                        break
                i += 1
            if pattern in src[body_start:i]:
                hits.add(m.group(1))
    return hits

rs = list(root.rglob("*.rs"))
health_fns = fn_defining('"/health"', rs)

# Everything chained BEFORE `.layer(... require_auth ...)` is wrapped by it.
# Slice from the start of the router statement up to that layer call and take
# every function named in between — carrier, for instance, merges its MCP router
# in between, so requiring the layer to sit adjacent to `router()` misses it.
boot = (root / "bootstrap.rs").read_text(encoding="utf-8", errors="ignore")
wrapped = set()
for m in re.finditer(r"require_auth", boot):
    head = boot[:m.start()]
    # Back up to the start of the statement building this router.
    stmt = head.rfind("let app")
    for kw in ("let protected", "let app"):
        k = head.rfind(kw)
        if k > stmt:
            stmt = k
    if stmt == -1:
        stmt = max(0, m.start() - 800)
    for call in re.finditer(r"([\w]+)\s*\(", head[stmt:]):
        name = call.group(1)
        if name.endswith("router") or name == "router":
            wrapped.add(name)

both = health_fns & wrapped
print("BEHIND_AUTH:" + ",".join(sorted(both)) if both else "OK")
PY
)

  if [[ "$verdict" == BEHIND_AUTH:* ]]; then
    echo "BEHIND AUTH  $svc — ${verdict#BEHIND_AUTH:}() defines /health and is wrapped by require_auth"
    fail=$((fail + 1))
  fi
done

if [ "$fail" -gt 0 ]; then
  echo ""
  echo "$fail service(s) have an unreachable or missing /health."
  echo "A probe cannot send a JWT. Define the observability routes in their own"
  echo "router and .merge() it AFTER the require_auth layer — see how hub-ops or"
  echo "carrier compose theirs."
  exit 1
fi

echo "Every service defines /health outside the auth layer."
