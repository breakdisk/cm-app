#!/usr/bin/env bash
# field-ops must never read into `offer_card`.
#
# It stores the blob and returns it whole. The moment a query reaches inside,
# this tier knows what a product's job *is* and has stopped being
# product-agnostic -- which ADR-0015 says is the property that disqualifies
# something from being a platform tier.
#
# Any JSON path or containment operator against that column is the failure this
# guards. The column is written and read as a whole value, so there is no
# legitimate use of these.
set -euo pipefail

if grep -rInE "offer_card[[:space:]]*(->>|->|#>>|#>|@>|<@|\?\||\?&|\?)" \
     --include='*.rs' --include='*.sql' services/field-ops/; then
  echo
  echo "ERROR: field-ops reads into offer_card."
  echo "That column is opaque by design -- see ADR-0015 and migration 0008."
  echo "If a product needs field-ops to act on something, it belongs in a"
  echo "first-class column this tier declares and owns."
  exit 1
fi

echo "OK: offer_card is still opaque to field-ops."
