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

# Constants named in a topic list a service subscribes to.
#
# This has now been wrong twice, each time by assuming a spelling:
#
#   consumer.subscribe(&[topics::X])                 -- rdkafka, directly
#   KafkaConsumer::new(brokers, group, &[topics::X]) -- the libs/events wrapper
#   KafkaConsumer::new(brokers, group, &[X])         -- the same, imported bare
#
# The second cost the whole online-payment path (2026-08-29). The third hid
# `pod.pickup.captured` -- subscribed by payments, order-intake and
# delivery-experience, published by pod, and created by nobody -- while this
# script printed "all subscribed topics are created" (2026-08-31).
#
# There was a second, quieter hole in the same line: the `subscribe(...)` arm
# was matched WITHOUT `-z`, so it could only ever see a single-line call. Every
# real `.subscribe(&[` in this repo is written across lines, so that arm matched
# almost nothing and the coverage came from the wrapper arm alone.
#
# So stop pattern-matching the spelling. Pull every all-caps identifier out of
# the argument lists and keep the ones topics.rs actually defines. A fourth
# spelling is then already covered, because the constant name is the thing that
# cannot change. `-z` on both arms so `[^]]*` may cross newlines.
known_names=$(echo "$consts" | awk '{print $1}' | sort -u)
subscribed=$( { \
    grep -rhzoE 'subscribe\(&\[[^]]*\]' services/*/src --include='*.rs' 2>/dev/null | tr '\0' '\n'; \
    grep -rhzoE 'KafkaConsumer::new\([^)]*\)' services/*/src --include='*.rs' 2>/dev/null | tr '\0' '\n'; \
  } | grep -oE '[A-Z][A-Z_0-9]{2,}' | sort -u \
    | grep -Fx "$known_names" | sort -u)

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
