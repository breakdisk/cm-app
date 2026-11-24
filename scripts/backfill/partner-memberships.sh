#!/usr/bin/env bash
# Backfill identity.partner_memberships for partner users that existed before
# the `pid` claim.
#
# Why this exists: once AUTH__ENFORCE_PARTNER_SCOPE=true, a partner-role token
# with no `pid` is refused. A partner who has no membership row would be locked
# out of the portal. Run this, wait one full token lifetime, and only then
# enforce (see docs/superpowers/plans/...-phase-0-partner-safety.md, Task 0.17).
#
# What it matches: a user holding the `partner` role whose lower-cased email
# equals a carrier's lower-cased `contact_email` in the same tenant. That is the
# same rule the partner portal's GET /v1/carriers/me already applies, so it
# grants exactly the binding the portal assumes today. With --riders it also
# binds `driver_ops.drivers` rows that carry a carrier_id.
#
# Safety: dry-run unless --apply. A user who maps to more than one carrier is
# reported and NOT inserted, because picking one would scope them to someone
# else's data. A user who already has an active membership is never changed.
# Idempotent: re-running adds nothing.
#
# The three schemas live in three databases in production, so this exports from
# each and joins locally rather than querying across them.
#
#   PSQL          psql command prefix (default: psql). On the VPS:
#                 PSQL="docker exec -i logisticos-postgres psql -U logisticos"
#   IDENTITY_DB / CARRIER_DB / DRIVER_OPS_DB   database names
#   TENANT_ID     restrict every read to one tenant (optional)
#
# Usage: partner-memberships.sh [--apply] [--riders]
set -euo pipefail

APPLY=0
RIDERS=0
for arg in "$@"; do
  case "$arg" in
    --apply)   APPLY=1 ;;
    --dry-run) APPLY=0 ;;
    --riders)  RIDERS=1 ;;
    -h|--help) sed -n '2,/^set -euo/p' "$0" | sed 's/^# \{0,1\}//' | head -n -1; exit 0 ;;
    *) echo "unknown argument: $arg" >&2; exit 2 ;;
  esac
done

PSQL=${PSQL:-psql}
IDENTITY_DB=${IDENTITY_DB:-svc_identity}
CARRIER_DB=${CARRIER_DB:-svc_carrier}
DRIVER_OPS_DB=${DRIVER_OPS_DB:-svc_driver_ops}
TENANT_ID=${TENANT_ID:-}

UUID_RE='^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
if [ -n "$TENANT_ID" ] && ! [[ "$TENANT_ID" =~ $UUID_RE ]]; then
  echo "TENANT_ID is not a lower-case uuid: $TENANT_ID" >&2; exit 2
fi
tenant_filter() { [ -n "$TENANT_ID" ] && printf "AND tenant_id = '%s'" "$TENANT_ID" || true; }

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# q <db> <sql>: unaligned, no header, '|' separated.
q() {
  # PSQL is deliberately word-split: it is a command prefix.
  # shellcheck disable=SC2086
  $PSQL -d "$1" -v ON_ERROR_STOP=1 -At -F '|' -c "$2"
}

q "$CARRIER_DB" "SELECT tenant_id, id, lower(btrim(contact_email)) FROM carrier.carriers WHERE true $(tenant_filter)" > "$work/carriers"
q "$IDENTITY_DB" "SELECT tenant_id, id, lower(btrim(email)) FROM identity.users WHERE 'partner' = ANY(roles) AND is_active $(tenant_filter)" > "$work/users"
q "$IDENTITY_DB" "SELECT tenant_id, user_id, carrier_id FROM identity.partner_memberships WHERE status = 'active' $(tenant_filter)" > "$work/existing"
: > "$work/riders"
if [ "$RIDERS" -eq 1 ]; then
  q "$DRIVER_OPS_DB" "SELECT tenant_id, user_id, carrier_id FROM driver_ops.drivers WHERE carrier_id IS NOT NULL $(tenant_filter)" > "$work/riders"
fi

# Candidate pairs: tenant|user|carrier|role. Carriers with no matching user go
# to unmatched. Matching key is tenant + lower-cased email.
awk -F'|' -v unmatched="$work/unmatched" '
  # FILENAME, not NR == FNR: with an empty first file NR == FNR stays true for
  # the second file too, and every row would be read as the wrong input.
  FILENAME == ARGV[1] { u[$1 "|" $3] = u[$1 "|" $3] " " $2; next }
  {
    key = $1 "|" $3
    n = split(u[key], ids, " ")
    if (n == 0) { print $1 "|" $2 "|" $3 > unmatched; next }
    for (i = 1; i <= n; i++) print $1 "|" ids[i] "|" $2 "|partner"
  }
' "$work/users" "$work/carriers" > "$work/candidates"
: >> "$work/unmatched"
awk -F'|' '{ print $1 "|" $2 "|" $3 "|store_rider" }' "$work/riders" >> "$work/candidates"

# Drop anything that is not a well-formed uuid triple before it nears SQL.
invalid=0
: > "$work/clean"
while IFS='|' read -r t u c r; do
  if [[ "$t" =~ $UUID_RE && "$u" =~ $UUID_RE && "$c" =~ $UUID_RE ]]; then
    printf '%s|%s|%s|%s\n' "$t" "$u" "$c" "$r" >> "$work/clean"
  else
    invalid=$((invalid + 1))
  fi
done < "$work/candidates"

# Group per user. count>1 distinct carriers = multiple_carriers (skipped).
awk -F'|' '
  { k = $1 "|" $2; c = $3
    if (!((k, c) in seen)) { seen[k, c] = 1; n[k]++; list[k] = list[k] (list[k] == "" ? "" : ",") c; role[k] = $4 } }
  END { for (k in n) print k "|" n[k] "|" list[k] "|" role[k] }
' "$work/clean" | sort > "$work/grouped"

# Split the single-carrier users into to-insert, already-present, held elsewhere.
awk -F'|' -v out_multi="$work/multi" -v out_insert="$work/insert" \
          -v out_present="$work/present" -v out_other="$work/other" '
  FILENAME == ARGV[1] { have[$1 "|" $2] = $3; next }
  {
    k = $1 "|" $2; count = $3; carrier = $4; role = $5
    if (count > 1) { print k "|" carrier > out_multi; next }
    if (k in have) {
      if (have[k] == carrier) print k "|" carrier > out_present
      else                    print k "|" carrier "|has:" have[k] > out_other
      next
    }
    print k "|" carrier "|" role > out_insert
  }
' "$work/existing" "$work/grouped"
for f in multi insert present other; do : >> "$work/$f"; done

matched=$(cat "$work/insert" "$work/present" "$work/other" | wc -l | tr -d ' ')
multi=$(wc -l < "$work/multi" | tr -d ' ')
unmatched=$(wc -l < "$work/unmatched" | tr -d ' ')
to_insert=$(wc -l < "$work/insert" | tr -d ' ')
present=$(wc -l < "$work/present" | tr -d ' ')
other=$(wc -l < "$work/other" | tr -d ' ')

echo "matched=$matched unmatched_email=$unmatched multiple_carriers=$multi"
echo "  would_insert=$to_insert already_present=$present held_by_other_carrier=$other skipped_invalid=$invalid"
if [ "$unmatched" -gt 0 ]; then
  echo "carriers with no matching partner user (tenant|carrier|contact_email):"; sed 's/^/  /' "$work/unmatched"
fi
if [ "$multi" -gt 0 ]; then
  echo "users matching more than one carrier, NOT inserted (tenant|user|carriers):"; sed 's/^/  /' "$work/multi"
fi
if [ "$other" -gt 0 ]; then
  echo "users already active for a different carrier, left alone (tenant|user|wanted|has):"; sed 's/^/  /' "$work/other"
fi

if [ "$APPLY" -ne 1 ]; then
  echo "dry-run: nothing written. Re-run with --apply to insert $to_insert membership(s)."
  exit 0
fi
if [ "$to_insert" -eq 0 ]; then
  echo "apply: nothing to insert."
  exit 0
fi

values=$(awk -F'|' '{ printf "%s('"'"'%s'"'"','"'"'%s'"'"','"'"'%s'"'"','"'"'%s'"'"','"'"'active'"'"')", (NR > 1 ? "," : ""), $1, $2, $3, $4 }' "$work/insert")
inserted=$(q "$IDENTITY_DB" "WITH ins AS (INSERT INTO identity.partner_memberships (tenant_id, user_id, carrier_id, role, status) VALUES $values ON CONFLICT DO NOTHING RETURNING 1) SELECT count(*) FROM ins")
echo "apply: inserted=$inserted (of $to_insert candidates; the rest already existed)"
