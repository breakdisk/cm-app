#!/usr/bin/env python3
"""Fail when a route under a tracked prefix is served by a service but missing
from its OpenAPI contract.

Tracked prefixes live in scripts/openapi-tracked-prefixes.txt. Only those are
checked: the existing specs have drifted from the live routes, and a check that
fails on old drift gets switched off. Usage: python scripts/check-openapi-routes.py
Exit 0 = every tracked route is documented, 1 = at least one is not.
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TRACKED = ROOT / "scripts" / "openapi-tracked-prefixes.txt"

# .route("/v1/foo/:id", ...) - the path is the first string literal argument,
# possibly on the next line.
ROUTE_RE = re.compile(r'\.route\(\s*"(/[^"]*)"')
SPEC_PATH_RE = re.compile(r"^  (/[^\s:]*):\s*$")


def normalise(path: str) -> str:
    """Make axum's /:id and OpenAPI's /{id} comparable, and drop a trailing slash."""
    path = re.sub(r"/:[A-Za-z_][A-Za-z0-9_]*", "/{}", path)
    path = re.sub(r"/\{[^}]*\}", "/{}", path)
    return path.rstrip("/") or "/"


def tracked_prefixes():
    out = []
    for line in TRACKED.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        service, prefix = (part.strip() for part in line.split("|", 1))
        out.append((service, prefix))
    return out


def served_routes(service: str):
    src = ROOT / "services" / service / "src"
    found = set()
    for rs in src.rglob("*.rs"):
        text = rs.read_text(encoding="utf-8", errors="replace")
        for match in ROUTE_RE.finditer(text):
            path = match.group(1)
            found.add(normalise(path))
            # Several services declare protected routes relative to a nested
            # `/v1` (identity's `/partner-memberships/me`). Treating every
            # relative route as also living under /v1 over-approximates, which
            # is the safe direction for a check that must not miss one.
            if not path.startswith("/v1"):
                found.add(normalise("/v1" + path))
    return found


def documented_routes(service: str):
    spec = ROOT / "docs" / "api" / "openapi" / f"{service}.yaml"
    if not spec.exists():
        return None
    in_paths = False
    found = set()
    for line in spec.read_text(encoding="utf-8").splitlines():
        if line.startswith("paths:"):
            in_paths = True
            continue
        if in_paths and re.match(r"^\S", line):
            break  # next top-level key
        if in_paths:
            m = SPEC_PATH_RE.match(line)
            if m:
                # Specs are relative to the server URL, which ends in /v1.
                found.add(normalise("/v1" + m.group(1)))
    return found


def main() -> int:
    missing = []
    by_service = {}
    for service, prefix in tracked_prefixes():
        by_service.setdefault(service, []).append(normalise(prefix))

    for service, prefixes in sorted(by_service.items()):
        documented = documented_routes(service)
        if documented is None:
            print(f"no contract at docs/api/openapi/{service}.yaml for tracked service")
            missing.append((service, "<spec file>"))
            continue
        for route in sorted(served_routes(service)):
            if any(route == p or route.startswith(p + "/") for p in prefixes):
                if route not in documented:
                    missing.append((service, route))

    for service, route in missing:
        print(f"UNDOCUMENTED {service}: {route}")
    if missing:
        print(f"\n{len(missing)} tracked route(s) missing from their OpenAPI contract.")
        return 1
    print("openapi route check: every tracked route is documented")
    return 0


if __name__ == "__main__":
    sys.exit(main())
