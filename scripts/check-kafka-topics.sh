#!/usr/bin/env bash
# Every topic a service subscribes to must be pre-created.
#
# A Kafka topic is created by its first PUBLISH. A consumer that subscribes
# first logs `UnknownTopicOrPartition` and does not recover on its own — it sits
# subscribed to nothing until the process is restarted after the topic exists.
# So whether a feature works depends on which of the publisher or the consumer
# happened to start first, which is not a property anyone can reason about.
#
# `scripts/create-kafka-topics.sh` removes that coin-flip by creating them up
# front. But it is a hand-maintained shell list living apart from the Rust
# constants, and it drifted: on 2026-08-09, 9 of the 14 subscribed topics did
# not exist on the VPS broker, and 10 were absent from the script. Whole
# features — the gig offer waves, customs clearance, hub→dispatch handoff, CRM
# auto-enrolment — were inert with no error beyond one line at startup.
#
# This check fails when a `topics::CONSTANT` that some service subscribes to is
# missing from the creation script. It reads the same two files a reviewer would
# and needs no cluster.
set -euo pipefail

cd "$(dirname "$0")/.."

TOPICS_RS="libs/events/src/topics.rs"
CREATE_SH="scripts/create-kafka-topics.sh"

[ -f "$TOPICS_RS" ] || { echo "missing $TOPICS_RS"; exit 1; }
[ -f "$CREATE_SH" ] || { echo "missing $CREATE_SH"; exit 1; }

# CONSTANT_NAME topic.name
consts=$(grep -oE 'pub const [A-Z_0-9]+: *&str = "[a-z0-9._]+"' "$TOPICS_RS" \
         | sed 's/pub const //; s/: *&str = "/ /; s/"$//')

# Constants named inside any subscribe(&[...]) across the services.
subscribed=$(grep -rhoE 'subscribe\(&\[[^]]*\]' services/*/src --include='*.rs' 2>/dev/null \
             | grep -oE 'topics::[A-Z_0-9]+' | sed 's/topics:://' | sort -u)

missing=0
while read -r name; do
  [ -n "$name" ] || continue
  topic=$(echo "$consts" | awk -v n="$name" '$1 == n {print $2}')
  if [ -z "$topic" ]; then
    echo "WARN  topics::$name is subscribed to but not defined in $TOPICS_RS"
    continue
  fi
  # Match the quoted argument, so a topic that is merely mentioned in a comment
  # does not count as created.
  if ! grep -qE "create_topic +\"$(echo "$topic" | sed 's/\./\\./g')\"" "$CREATE_SH"; then
    echo "MISSING  $topic  (topics::$name)"
    missing=$((missing + 1))
  fi
done <<EOF
$subscribed
EOF

if [ "$missing" -gt 0 ]; then
  echo ""
  echo "$missing subscribed topic(s) are not created by $CREATE_SH."
  echo "A consumer subscribing before the first publish gets UnknownTopicOrPartition"
  echo "and stays broken until it is restarted. Add a create_topic line for each."
  exit 1
fi

echo "All subscribed topics are created by $CREATE_SH."
