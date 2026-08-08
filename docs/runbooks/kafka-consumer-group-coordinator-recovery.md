# Runbook: Kafka consumer groups dead — `__consumer_offsets` topic-ID mismatch

**Severity: platform-wide.** Every `subscribe()`-based consumer stops receiving,
silently. Producers are unaffected, so the system looks healthy from the write
side while nothing downstream reacts.

**Observed 2026-08-08 on `75.119.138.135`. Onset 2026-08-04 16:47.**

---

## Symptoms

- A service's Kafka consumer logs no errors and never exits, but handles nothing.
- `kafka-consumer-groups --list` returns empty; `--describe` fails with
  `TimeoutException: Timed out waiting for a node assignment`.
- `kafka-console-consumer --topic X --from-beginning` returns
  `Processed a total of 0 messages`, **but the same read with
  `--partition 0 --offset earliest` returns the messages fine.**
- `kafka-topics --describe` and `GetOffsetShell` both work.

That last pair is the signature: **assign works, subscribe does not.** Anything
that needs the group coordinator hangs; everything else is healthy.

## Diagnosis

```bash
docker logs logisticos-kafka 2>&1 | grep -c "does not match the topic ID for partition __consumer_offsets"
```

A non-zero count (50 = every partition) confirms it. The full line reads:

```
ERROR [Broker id=1] Topic ID in memory: pIUNPJZ6RpuVqnKV-ceZuQ does not match
the topic ID for partition __consumer_offsets-27 received: tDXAqjQCRx6vY13YGKdeUg
```

The on-disk `__consumer_offsets` partitions carry a topic ID from a previous
cluster incarnation; ZooKeeper has registered a new one. The broker refuses to
load the mismatched partitions, so the group coordinator can never come up —
even though `kafka-topics --describe __consumer_offsets` reports all 50
partitions with a healthy leader and ISR. **Do not trust that describe output.**

This is residue from the ZooKeeper cluster-ID reset that caused the crashloop
fixed in `bf512c70`. The crashloop was resolved; this was left behind.

## Blast radius

Every consumer on the platform. Confirmed 2026-08-08 by
`docker logs logisticos-<svc> --since 72h | grep -c consumed` returning 0 for
engagement, order-intake, pod and driver-ops.

Concretely, while this is broken: shipment status never advances from Kafka
events, notifications are never sent, COD and vendor ledgers are never credited,
and POD-driven invoicing does not fire. Producers keep writing, so the backlog
is intact and replayable — nothing is lost, only unprocessed.

## Fix

Deleting `__consumer_offsets` from disk lets the broker recreate it with the
ZooKeeper-registered topic ID.

> **This discards every committed offset on the platform.** Do not run the
> delete without the offset-positioning step that follows it, or every consumer
> restarts at `auto.offset.reset=earliest` and **replays its entire topic**.
> `logisticos.driver.delivery.completed` and `logisticos.pod.captured` drive
> ledger credits and receipts — replaying those double-credits money and
> re-sends customer messages. The topics are small (largest is ~1.4 MB of GPS
> breadcrumbs) so the replay is fast, which makes it more dangerous, not less.

```bash
# 1. Stop the consumers first, so nothing rejoins mid-repair.
cd /etc/dokploy/compose/oscargomarketnet-logisticosbackend-pqfh0u/code/
docker compose stop engagement order-intake pod driver-ops dispatch omnideliv \
                   marketing business-logic analytics delivery-experience

# 2. Stop the broker.
docker compose stop kafka

# 3. Remove only the offsets topic's log directories. Nothing else.
docker run --rm -v <kafka-data-volume>:/data alpine \
  sh -c 'rm -rf /data/__consumer_offsets-*'

# 4. Start the broker and wait for the coordinator.
docker compose start kafka
sleep 30
docker logs logisticos-kafka 2>&1 | grep "GroupCoordinator" | tail -2
#    expect "Startup complete" with no topic-ID errors following it

# 5. Position every group at the tail BEFORE starting its consumer, so the
#    backlog is not replayed. Repeat per group id.
docker exec logisticos-kafka kafka-consumer-groups \
  --bootstrap-server kafka:29092 \
  --group <group-id> --all-topics --reset-offsets --to-latest --execute
```

Step 5 is the judgement call. `--to-latest` skips the 4-day backlog that
accumulated while consumers were dead — those events are genuinely unprocessed,
so skipping them means status flips and ledger credits from that window never
happen and need reconciling by hand. `--to-earliest` processes them but also
reprocesses everything older. **Neither is safe blind:** decide per topic, and
prefer `--to-datetime 2026-08-04T16:47:00.000` for the topics whose backlog you
actually want, which replays only the outage window.

## Verify

```bash
docker exec logisticos-kafka kafka-consumer-groups --bootstrap-server kafka:29092 --list
```

Groups appear. Then the end-to-end check:

```bash
bash scripts/omnideliv-smoke.sh    # with TOKEN and COURIER_TOKEN set
```

Step 7 passing — a courier claim advancing the order to `collecting` — is the
proof that a real consumer group is receiving again.

## Prevention

- **Pre-create topics.** `scripts/create-kafka-topics.sh` now includes
  `fieldops.courier`. A topic that only exists after the first publish leaves a
  consumer that started first stuck on `UnknownTopicOrPartition` with no
  recovery, which looks exactly like this incident but is not.
- **Alert on group liveness, not broker health.** The broker reported healthy
  throughout, its healthcheck passed, and `kafka-topics --describe` was clean.
  The only signal was consumers doing nothing. A check that
  `kafka-consumer-groups --list` is non-empty would have caught this in minutes
  rather than four days.
