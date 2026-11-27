#!/usr/bin/env bash
# Every partition of every topic must answer an offset request.
#
# Run this ON THE BROKER HOST (it uses `docker exec`). It is the check that
# tells a healthy topic from one with an orphaned partition. `kafka-topics
# --list` and `--describe` both look fine for a broken topic: the leader is
# set, the ISR is full. Only an offset request reveals that one partition's
# on-disk log belongs to an older incarnation of the topic (a topic-ID
# mismatch), so the broker refuses to serve it.
#
# Why it matters: producers key by tenant_id. Every event for a tenant that
# hashes to a broken partition fails with NotLeaderOrFollower, is retried
# inside message.timeout.ms, and is dropped. The publishing service logs one
# error and carries on, so nothing downstream ever hears about it.
#
# 2026-11-25: 12 topics had partition 0 orphaned, including
# order.shipment.created, pod.captured, identity.user.created and
# task.assigned. Repair: docs/runbooks/kafka-operations.md, "Orphaned
# partitions".
#
# Usage:  scripts/check-kafka-partitions.sh [container] [bootstrap]
# Exit:   0 if every partition reports an offset, 1 if any does not, and
#         2 if the broker cannot be queried at all.
set -euo pipefail

CONTAINER="${1:-logisticos-kafka}"
BOOTSTRAP="${2:-localhost:9092}"

# One request for every partition of every topic. Asking per topic takes
# several seconds for each broken one and times out over ~80 topics.
if ! out=$(docker exec "$CONTAINER" kafka-get-offsets --bootstrap-server "$BOOTSTRAP" --time -1 2>&1); then
  echo "cannot query $CONTAINER: $out" >&2
  exit 2
fi

ok=$(printf '%s\n' "$out" | grep -cE '^[^ ]+:[0-9]+:[0-9]+$' || true)
broken=$(printf '%s\n' "$out" | grep -oE 'topic-partition [^ ]+' | sed 's/topic-partition //' | sort -u || true)

if [ "$ok" = "0" ] && [ -z "$broken" ]; then
  echo "no partitions reported. Is $CONTAINER a Kafka broker?" >&2
  exit 2
fi

if [ -n "$broken" ]; then
  echo "BROKEN partitions ($(printf '%s\n' "$broken" | wc -l)), $ok healthy:"
  printf '  %s\n' $broken
  echo "Confirm the cause (grep after the last broker start, not the whole log):"
  echo "  docker logs --since 1h $CONTAINER 2>&1 | grep 'does not match the topic ID'"
  exit 1
fi

echo "OK: all $ok partitions report offsets"
