#!/usr/bin/env python3
"""Every handler a partner-reachable role can call must say how it is scoped.

Why this exists: a `partner` token used to read every shipment, wallet, manifest
and rider in its tenant, and a store rider's plain `driver` role reached the
dispatch queue, because each handler checked only that the caller held some
permission and nothing said whose data that permission opened. A handler added
next year with `shipments:read` and no scope check would repeat it.

What it checks, with no dependencies beyond the standard library:

  1. Find every async handler that takes the caller's claims and names a
     permission that partner, store or driver roles hold (read from
     libs/auth/src/rbac.rs, so a new role or grant is picked up on its own).
  2. Each such handler must appear in scripts/partner-scope-inventory.yaml with
     a status and a reason. An unlisted one fails the build.
  3. The status is verified against the source, not just trusted:
       denied         the body must call `require_unscoped(` (tenant operations)
       scoped         the body must use partner scoping (`partner_access`,
                      `carrier_filter`, `manifest_scope`, `roster_scope`, ...)
       not_sensitive  a reason is required; nothing to verify
  4. An inventory entry for a handler that no longer exists, or no longer needs
     one, fails too, so the list stays true.

Usage: python scripts/check-partner-scope.py [--list]
Exit 0 = every handler is classified and verified, 1 = something is not.
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
RBAC = ROOT / "libs" / "auth" / "src" / "rbac.rs"
INVENTORY = ROOT / "scripts" / "partner-scope-inventory.yaml"

# Roles whose holders are not the tenant's own staff: a partner or store, and the
# plain driver role that a store's rider holds. `store_*` is matched by prefix so
# a new store role needs no edit here.
PARTNER_REACHABLE_ROLES = ("partner", "driver")
PARTNER_REACHABLE_PREFIX = "store_"

SCOPE_MARKERS = (
    "partner_access(", "carrier_filter(", "manifest_scope(", "roster_scope(",
    "PartnerAccess", "require_unscoped(",
    # carrier resolves a partner to its own carrier record by contact email.
    "get_by_email(", "load_readable_carrier(", "load_writable_carrier(", "CarrierAuthority",
)

FN_RE = re.compile(r"^(?:pub(?:\([a-z]+\))?\s+)?async\s+fn\s+(\w+)", re.M)
PERM_RE = re.compile(r"permissions::([A-Z][A-Z0-9_]*)")
CONST_RE = re.compile(r'pub const ([A-Z][A-Z0-9_]*):\s*&str\s*=\s*"([^"]+)"')
ENTRY_RE = re.compile(r"^(\S+::\w+):\s*(scoped|denied|not_sensitive)\s*#\s*(\S.*)$")


def permission_values():
    """NAME -> 'resource:action' for every constant in rbac.rs."""
    return dict(CONST_RE.findall(RBAC.read_text(encoding="utf-8")))


def role_permissions(values):
    """role -> set of permission strings, from default_permissions_for_role."""
    text = RBAC.read_text(encoding="utf-8")
    start = text.index("pub fn default_permissions_for_role")
    body = text[start:]
    roles = {}
    for m in re.finditer(r'"([a-z_]+)"\s*=>\s*vec!\[(.*?)\]', body, re.S):
        names = PERM_RE.findall(m.group(2))
        roles[m.group(1)] = {values[n] for n in names if n in values}
    return roles


def sensitive_permissions(values):
    roles = role_permissions(values)
    held = set()
    for role, perms in roles.items():
        if role in PARTNER_REACHABLE_ROLES or role.startswith(PARTNER_REACHABLE_PREFIX):
            held |= perms
    return held


def handlers():
    """(key, body, permission strings used) for every claims-taking async fn."""
    values = permission_values()
    out = []
    for rs in sorted((ROOT / "services").glob("*/src/**/*.rs")):
        text = rs.read_text(encoding="utf-8", errors="replace")
        lines = text.splitlines()
        for m in FN_RE.finditer(text):
            start_line = text.count("\n", 0, m.start())
            # A top-level fn ends at the next line that is exactly "}".
            end_line = next((i for i in range(start_line + 1, len(lines)) if lines[i].rstrip() == "}"), len(lines) - 1)
            body = "\n".join(lines[start_line : end_line + 1])
            if "claims" not in body[:600] and "AuthClaims" not in body[:600]:
                continue
            used = {values[n] for n in PERM_RE.findall(body) if n in values}
            # A handler that delegates to the scoping helpers may name no
            # permission itself (the manifest and roster do), but it is exactly
            # the kind that must stay classified and verified.
            if any(marker in body for marker in SCOPE_MARKERS):
                used = used | {"(partner scoping)"}
            if used:
                key = f"{rs.relative_to(ROOT).as_posix()}::{m.group(1)}"
                out.append((key, body, used))
    return out


def load_inventory():
    entries, problems = {}, []
    for n, raw in enumerate(INVENTORY.read_text(encoding="utf-8").splitlines(), 1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        m = ENTRY_RE.match(line)
        if not m:
            problems.append(f"{INVENTORY.name}:{n}: not `path::fn: status  # reason` : {raw!r}")
            continue
        entries[m.group(1)] = (m.group(2), m.group(3).strip())
    return entries, problems


def main() -> int:
    values = permission_values()
    sensitive = sensitive_permissions(values)
    found = {}
    for key, body, used in handlers():
        reach = (used & sensitive) | ({"(partner scoping)"} & used)
        if reach:
            found[key] = (body, reach)

    if "--list" in sys.argv:
        for key in sorted(found):
            print(f"{key}  [{', '.join(sorted(found[key][1]))}]")
        print(f"\n{len(found)} partner-reachable handler(s)")
        return 0

    inventory, problems = load_inventory()
    for key in sorted(found):
        body, reach = found[key]
        if key not in inventory:
            problems.append(f"UNCLASSIFIED {key}  (partner-reachable via {', '.join(sorted(reach))})")
            continue
        status, _why = inventory[key]
        if status == "denied" and "require_unscoped(" not in body:
            problems.append(f"DENIED_WITHOUT_GUARD {key}: marked denied but never calls require_unscoped(")
        if status == "scoped" and not any(marker in body for marker in SCOPE_MARKERS):
            problems.append(f"SCOPED_WITHOUT_SCOPING {key}: marked scoped but uses no partner scoping")
    for key in sorted(inventory):
        if key not in found:
            problems.append(f"STALE {key}: in the inventory but no longer a partner-reachable handler")

    for p in problems:
        print(p)
    if problems:
        print(f"\n{len(problems)} problem(s). Classify each new handler in scripts/partner-scope-inventory.yaml.")
        return 1
    print(f"partner scope check: {len(found)} partner-reachable handlers, all classified and verified")
    return 0


if __name__ == "__main__":
    sys.exit(main())
