#!/usr/bin/env python3
"""Every handler a partner-reachable role can call must say how it is scoped.

Why this exists: a `partner` token used to read every shipment, wallet, manifest
and rider in its tenant, and a store rider's plain `driver` role reached the
dispatch queue, because each handler checked only that the caller held some
permission and nothing said whose data that permission opened. A handler added
next year with `shipments:read` and no scope check would repeat it.

What it checks, with no dependencies beyond the standard library:

  1. Find every fn, at any indentation and including impl methods, whose full
     parameter list takes `Claims`, `AuthClaims` or `McpContext` and whose body
     names a permission that partner, store or driver roles hold (read from
     libs/auth/src/rbac.rs, so a new role or grant is picked up on its own).
     A permission is named as `permissions::X`, or as the string or const
     argument of `has_permission(...)` / `require_permission(...)`. Code under
     `#[cfg(test)]` is ignored.
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

FN_RE = re.compile(r"^[ \t]*(?:pub(?:\([a-z: ]+\))?\s+)?(?:const\s+)?(?:async\s+)?(?:unsafe\s+)?fn\s+(\w+)", re.M)
CALLER_RE = re.compile(r"\b(?:Claims|AuthClaims|McpContext)\b")
PERM_RE = re.compile(r"permissions::([A-Z][A-Z0-9_]*)")
PERM_CALL_RE = re.compile(r"\b(?:has_permission|require_permission)\s*\(")
RAW_STR_RE = re.compile(r'b?r(#*)"')
CHAR_RE = re.compile(r"'(?:\\.[^']*|[^\\'])'")
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


def _skip_literal(text, i):
    """If a comment, string, raw string or char literal starts at i, return the
    index after it, else None. Lets bracket matching ignore their contents."""
    n = len(text)
    c = text[i]
    if text.startswith("//", i):
        j = text.find("\n", i)
        return n if j < 0 else j
    if text.startswith("/*", i):
        j = text.find("*/", i + 2)
        return n if j < 0 else j + 2
    m = RAW_STR_RE.match(text, i)
    if m:
        end = text.find('"' + m.group(1), m.end())
        return n if end < 0 else end + 1 + len(m.group(1))
    if c == '"':
        j = i + 1
        while j < n and text[j] != '"':
            j += 2 if text[j] == "\\" else 1
        return j + 1
    if c == "'":
        m = CHAR_RE.match(text, i)
        if m:  # a char literal; a lifetime such as 'a does not match
            return m.end()
    return None


def _match(text, i, open_c, close_c):
    """Index just past the bracket that closes the one at text[i]."""
    depth, n = 0, len(text)
    while i < n:
        skip = _skip_literal(text, i)
        if skip is not None:
            i = skip
            continue
        if text[i] == open_c:
            depth += 1
        elif text[i] == close_c:
            depth -= 1
            if depth == 0:
                return i + 1
        i += 1
    return n


def _test_regions(text):
    """Spans of `#[cfg(test)]` items, so test helpers are not read as handlers."""
    spans = []
    for m in re.finditer(r"#\[cfg\(test\)\]", text):
        j = text.find("{", m.end())
        semi = text.find(";", m.end())
        if j >= 0 and (semi < 0 or j < semi):
            spans.append((m.start(), _match(text, j, "{", "}")))
    return spans


def permissions_used(body, values):
    """Permissions a body names: `permissions::X` anywhere, plus the string or
    bare-const argument of has_permission(...) / require_permission(...)."""
    used = {values[n] for n in PERM_RE.findall(body) if n in values}
    for m in PERM_CALL_RE.finditer(body):
        args = body[m.end() : _match(body, m.end() - 1, "(", ")")]
        used.update(re.findall(r'"([a-z_]+:[a-z_-]+)"', args))
        for name in re.findall(r"\b([A-Z][A-Z0-9_]*)\b", args):
            if name in values:
                used.add(values[name])
    return used


def handlers():
    """(key, body, permission strings used) for every fn that takes the caller's
    claims, at any indentation (impl methods included)."""
    values = permission_values()
    merged = {}
    for rs in sorted((ROOT / "services").glob("*/src/**/*.rs")):
        text = rs.read_text(encoding="utf-8", errors="replace")
        tests = _test_regions(text)
        for m in FN_RE.finditer(text):
            if any(a <= m.start() < b for a, b in tests):
                continue
            paren = text.find("(", m.end())
            if paren < 0:
                continue
            params_end = _match(text, paren, "(", ")")
            if not CALLER_RE.search(text[paren:params_end]):
                continue
            # The body starts at the first `{` after the params; a `;` first
            # means a trait method with no body.
            i = params_end
            while i < len(text) and text[i] not in "{;":
                i += 1
            if i >= len(text) or text[i] == ";":
                continue
            body = text[m.start() : _match(text, i, "{", "}")]
            used = permissions_used(body, values)
            # A handler that delegates to the scoping helpers may name no
            # permission itself (the manifest and roster do), but it is exactly
            # the kind that must stay classified and verified.
            if any(marker in body for marker in SCOPE_MARKERS):
                used = used | {"(partner scoping)"}
            if used:
                key = f"{rs.relative_to(ROOT).as_posix()}::{m.group(1)}"
                prev = merged.get(key)
                merged[key] = (prev[0] + "\n" + body, prev[1] | used) if prev else (body, used)
    return [(k, b, u) for k, (b, u) in merged.items()]


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
