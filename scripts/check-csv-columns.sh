#!/usr/bin/env bash
#
# The CSV columns the console documents must be the ones the importer reads.
#
# Two hand-maintained lists in two languages. The console tells a vendor what to
# put in their file; the Rust parser decides what it actually accepts. Nothing
# connected them, so renaming a column in the parser — or adding one and
# forgetting the console — leaves a merchant following instructions that produce
# a rejected file, with the console insisting the file is right.
#
# This is the cheap half of the problem. It compares the *set* of column names,
# which catches a rename, an addition and a deletion. It cannot check that the
# prose describing each column is still true; that still needs a person.
#
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
RUST="$ROOT/services/omnideliv/src/application/csv_import.rs"
TSX="$ROOT/apps/merchant-portal/src/components/storefront/csv-format.tsx"

for f in "$RUST" "$TSX"; do
  if [ ! -f "$f" ]; then
    echo "FAIL: expected file is missing: ${f#"$ROOT/"}"
    echo "      If it moved, update this guard — deleting the guard because its"
    echo "      target moved is how the two lists drift in the first place."
    exit 1
  fi
done

# const COL_SKU: &str = "sku";
PARSER="$(grep -oE 'const COL_[A-Z_]+:[[:space:]]*&str[[:space:]]*=[[:space:]]*"[a-z_]+"' "$RUST" \
  | grep -oE '"[a-z_]+"' | tr -d '"' | sort -u)"

# { col: "sku", note: "…" }
CONSOLE="$(grep -oE '\{[[:space:]]*col:[[:space:]]*"[a-z_]+"' "$TSX" \
  | grep -oE '"[a-z_]+"' | tr -d '"' | sort -u)"

if [ -z "$PARSER" ]; then
  echo "FAIL: found no COL_* constants in csv_import.rs — the pattern this guard"
  echo "      greps for has changed, so it is no longer checking anything."
  exit 1
fi
if [ -z "$CONSOLE" ]; then
  echo "FAIL: found no documented columns in csv-format.tsx — same problem in the"
  echo "      other direction."
  exit 1
fi

echo "parser  : $(echo "$PARSER"  | tr '\n' ' ')"
echo "console : $(echo "$CONSOLE" | tr '\n' ' ')"

ONLY_PARSER="$(comm -23 <(echo "$PARSER") <(echo "$CONSOLE"))"
ONLY_CONSOLE="$(comm -13 <(echo "$PARSER") <(echo "$CONSOLE"))"

FAIL=0
if [ -n "$ONLY_PARSER" ]; then
  echo
  echo "FAIL: the importer reads columns the console never mentions:"
  echo "$ONLY_PARSER" | sed 's/^/        /'
  echo "      A vendor cannot use a column nobody told them about."
  FAIL=1
fi
if [ -n "$ONLY_CONSOLE" ]; then
  echo
  echo "FAIL: the console documents columns the importer ignores:"
  echo "$ONLY_CONSOLE" | sed 's/^/        /'
  echo "      This is the worse direction: the vendor fills the column in, the"
  echo "      import succeeds, and the data silently goes nowhere."
  FAIL=1
fi

[ "$FAIL" -ne 0 ] && exit 1

echo
echo "The console documents exactly the columns the importer reads."
