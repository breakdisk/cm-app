#!/usr/bin/env bash
# libs/agent-runtime must stay product-agnostic. If it names a product concept,
# it is no longer reusable by other products and the extraction has regressed.
set -euo pipefail

FORBIDDEN='shipment|driver|courier|vendor|merchant|dispatch|logistics|omnideliv|delivery|parcel'
TARGET='libs/agent-runtime/src'

if [ ! -d "$TARGET" ]; then
  echo "ERROR: $TARGET not found — run this from the repository root."
  exit 1
fi

# Pick a search tool explicitly rather than assuming ripgrep.
#
# This matters more than it looks: a missing tool inside an `if` condition exits
# 127, which `set -e` does not catch and the else-branch reads as "no matches".
# The check would print OK on any machine without rg — a green result proving
# nothing, which is worse than no check at all.
if command -v rg >/dev/null 2>&1; then
  SEARCH=(rg -i --type rust -n "$FORBIDDEN" "$TARGET")
elif command -v grep >/dev/null 2>&1; then
  SEARCH=(grep -rEin --include='*.rs' "$FORBIDDEN" "$TARGET")
else
  echo "ERROR: neither rg nor grep is available — cannot verify the boundary."
  exit 1
fi

# `|| true` keeps a clean no-match (exit 1) from tripping `set -e`; a real
# failure is distinguished below by inspecting the captured output.
MATCHES="$("${SEARCH[@]}" 2>/dev/null || true)"

if [ -n "$MATCHES" ]; then
  echo "$MATCHES"
  echo
  echo "ERROR: libs/agent-runtime references a product concept (matches above)."
  echo "The runtime must stay generic. Move product-specific code into the service."
  exit 1
fi

echo "OK: libs/agent-runtime is product-agnostic (checked with ${SEARCH[0]})."
