# Incident Response Runbook

**Owner:** Staff Platform Engineer / SRE Lead
**Last Reviewed:** 2026-03-17
**Applies To:** All LogisticOS production and staging environments
**Related ADRs:** ADR-0001 (Zero-downtime deployments), ADR-0005 (Observability)

---

## Table of Contents

1. [Severity Levels](#1-severity-levels)
2. [On-Call Rotation](#2-on-call-rotation)
3. [SEV1 Response Procedure](#3-sev1-response-procedure)
4. [SEV2 Response Procedure](#4-sev2-response-procedure)
5. [General Diagnosis Commands](#5-general-diagnosis-commands)
6. [Service-Specific Diagnosis](#6-service-specific-diagnosis)
7. [Rollback Procedures](#7-rollback-procedures)
8. [Database Incident Procedure](#8-database-incident-procedure)
9. [Kafka / MSK Incident Procedure](#9-kafka--msk-incident-procedure)
10. [Post-Incident Review Template](#10-post-incident-review-template)

---

## 1. Severity Levels

| Severity | Definition | Response Time (Acknowledge) | Resolution Target | Examples |
|----------|-----------|----------------------------|-------------------|---------|
| **SEV1** | Critical — complete service outage or data loss in progress. Revenue impact or SLA breach for all tenants. | **5 minutes** (24/7) | 1 hour | API gateway down, all dispatch failures, payment processing unavailable, database unreachable, active data breach |
| **SEV2** | Major — significant degradation affecting a large portion of tenants or a critical user-facing flow. | **15 minutes** (24/7) | 4 hours | Driver assignment latency > 5s, WhatsApp delivery failures > 20%, one-tenant complete outage, live tracking unavailable |
| **SEV3** | Minor — partial degradation, workaround available, single non-critical feature impacted. | **1 business hour** | 24 hours | Analytics dashboard slow, non-delivery notifications delayed, campaign send time degraded, single AI agent error-looping |
| **SEV4** | Low — cosmetic issue, edge-case bug, or non-urgent improvement with no user-facing impact. | **Next sprint** | Next sprint | UI misalignment, verbose log noise, Grafana panel misconfiguration |

### SEV Escalation Triggers

Automatically escalate a severity level if any of the following occur:
- Incident duration exceeds 50% of resolution target without a mitigation in place
- A second independent service failure is detected during the incident
- Tenant data integrity is called into question
- Media / regulatory inquiry is received

---

## 2. On-Call Rotation

### Schedule

On-call operates 24/7 across two tiers:

| Tier | Who | Coverage | Responsibilities |
|------|-----|----------|-----------------|
| **Primary On-Call** | SRE rotation (Staff Platform Engineer, Senior SRE) | 8-hour shifts × 3 (00:00–08:00, 08:00–16:00, 16:00–24:00 SGT) | First responder; acknowledge, triage, initiate mitigation |
| **Secondary On-Call** | Engineering Manager rotation | 24-hour backup | Escalation target if primary unresponsive after 5 minutes; stakeholder comms |
| **Domain Expert Pool** | Senior engineers per service domain | On-call by domain during business hours, paged on-demand off-hours | Deep investigation for service-specific incidents |

### Tools

- **PagerDuty** — alerting, escalation policy, on-call scheduling
  URL: `https://logisticos.pagerduty.com`
- **Slack** — incident channels (`#incidents-sev1`, `#incidents-sev2`, `#incidents-sev3`)
- **Grafana** — observability (`https://grafana.logisticos.internal`)
- **Runbook wiki** — this document + linked service runbooks
- **StatusPage** — customer-facing status (`https://status.logisticos.com`)

### Shift Handoff

Outgoing on-call must post a handoff note in `#on-call-handoff` before shift end:
- Open incidents and current status
- Any degraded services or elevated error rates
- Pending deployments or maintenance

---

## 3. SEV1 Response Procedure

### Step 1 — Acknowledge (< 5 minutes from alert)

```
PagerDuty app → Acknowledge alert
Post in #incidents-sev1: "SEV1 acknowledged by @{you} at {time}. Investigating."
```

### Step 2 — Create Incident Channel

```
/incident create sev1 in Slack
Channel naming: #inc-{YYYY-MM-DD}-{short-description}
Example: #inc-2026-03-17-dispatch-outage
```

Invite to the incident channel:
- Primary on-call + secondary on-call
- Relevant domain engineers (use `@dispatch-team`, `@payments-team`, etc.)
- Engineering Manager
- CTO (for extended SEV1s > 30 minutes)

### Step 3 — Notify Stakeholders

Within 10 minutes of acknowledgement:

```
# StatusPage update (customer-facing)
https://status.logisticos.com → Create Incident
- Title: "Investigating service degradation"
- Affected components: [select impacted services]
- Status: Investigating
- Do NOT speculate on cause or ETA in first update

# Internal Slack broadcast
Post in #engineering-all and #product-all:
"SEV1 in progress. Incident channel: #inc-{channel}. Updates every 15 minutes."
```

For payment-related SEV1s: also notify CFO and Finance on-call.
For data breach/security SEV1s: also notify CISO immediately.

### Step 4 — Triage (< 15 minutes from acknowledge)

Work through the triage checklist:

```
[ ] Identify affected service(s) — kubectl and Grafana
[ ] Determine blast radius — how many tenants are affected?
[ ] Check for recent deployments — did something just ship?
[ ] Check Kafka consumer lag — is event processing backed up?
[ ] Check RDS health — query latency, replication lag, disk
[ ] Check Redis — evictions, connection count, CPU
[ ] Review error rate in Grafana: logisticos-service-health dashboard
[ ] Check PagerDuty for correlated alerts across services
```

### Step 5 — Mitigate (target: < 30 minutes)

Apply the fastest available mitigation (not necessarily the root fix):
- **Rollback** — if caused by a recent deployment (see Section 7)
- **Traffic shift** — Istio canary reset to stable version
- **Scale up** — if resource exhaustion: `kubectl scale deployment`
- **Circuit break** — disable the failing feature via feature flag
- **Failover** — if RDS primary failed, confirm replica promotion

### Step 6 — Resolve and Update

```
# Mark resolved in PagerDuty
# Update StatusPage: "Issue resolved at {time}. Root cause investigation ongoing."
# Post in #incidents-sev1 and #engineering-all with resolution summary
```

### Step 7 — Post-Incident Review

Open a PIR ticket within 24 hours of resolution. Complete the PIR within 5 business days. See [Section 10](#10-post-incident-review-template).

---

## 4. SEV2 Response Procedure

Follow the same steps as SEV1 with these differences:
- Acknowledge SLA: 15 minutes
- Use `#incidents-sev2` channel; no dedicated incident channel required unless duration > 1 hour
- StatusPage update optional for internal-only impact; required if customer-facing
- PIR optional; required if the incident recurs within 30 days
- Secondary on-call notified but CTO notification only if escalating to SEV1

---

## 5. General Diagnosis Commands

> All commands assume `kubectl` is configured with the appropriate EKS cluster context.
> Set your context: `kubectl config use-context logisticos-production`

### Cluster Health

```bash
# Check all pods not in Running/Completed state across all LogisticOS namespaces
kubectl get pods -n logisticos-core \
  --field-selector=status.phase!=Running,status.phase!=Succeeded \
  -o wide

kubectl get pods -n logisticos-logistics \
  --field-selector=status.phase!=Running,status.phase!=Succeeded \
  -o wide

kubectl get pods -n logisticos-engagement \
  --field-selector=status.phase!=Running,status.phase!=Succeeded \
  -o wide

# Quick overview of all deployments and replica counts
kubectl get deployments -A -l app.kubernetes.io/part-of=logisticos

# Check node resource pressure
kubectl top nodes

# Check pod resource usage across a namespace
kubectl top pods -n logisticos-core --sort-by=cpu
```

### Recent Error Logs

```bash
# Tail last 5 minutes of logs from a specific pod, grep for errors
kubectl logs -n <namespace> <pod-name> --since=5m | grep -i "ERROR\|PANIC\|FATAL"

# Stream live logs from all pods in a deployment
kubectl logs -n <namespace> -l app=<deployment-name> -f --max-log-requests=10

# Get logs from a crashed/restarted pod (previous instance)
kubectl logs -n <namespace> <pod-name> --previous

# Check recent events in a namespace (CrashLoopBackOff, OOMKilled, etc.)
kubectl get events -n <namespace> --sort-by='.lastTimestamp' | tail -30

# Events for a specific pod
kubectl describe pod -n <namespace> <pod-name> | grep -A 20 "Events:"
```

### Namespace Reference

| Namespace | Services |
|-----------|---------|
| `logisticos-core` | identity, api-gateway, business-logic |
| `logisticos-logistics` | order-intake, dispatch, driver-ops, fleet, hub-ops, carrier, pod |
| `logisticos-engagement` | engagement, cdp, marketing |
| `logisticos-payments` | payments |
| `logisticos-ai` | ai-layer |
| `logisticos-analytics` | analytics |

### Istio Service Mesh

```bash
# Check VirtualService traffic splits (look for canary deployments)
kubectl get virtualservice -n logisticos-core -o yaml
kubectl get virtualservice -n logisticos-logistics -o yaml

# Check DestinationRule (circuit breakers, load balancing)
kubectl get destinationrule -A -l app.kubernetes.io/part-of=logisticos

# Inspect Istio proxy status for a pod (are sidecars in sync?)
istioctl proxy-status

# Check Envoy config for a specific pod (routing, clusters, listeners)
istioctl proxy-config routes -n logisticos-logistics <pod-name>

# Check mTLS status between services
istioctl authn tls-check <pod-name>.<namespace>

# View recent Istio access logs for a service (last 100 lines)
kubectl logs -n logisticos-logistics \
  -l app=dispatch \
  -c istio-proxy \
  --tail=100 | jq 'select(.response_code >= 500)'
```

### Kafka Consumer Lag

```bash
# Exec into any Kafka client pod to run consumer group commands
kubectl exec -it -n logisticos-core kafka-client-pod -- \
  kafka-consumer-groups.sh \
  --bootstrap-server ${KAFKA_BOOTSTRAP} \
  --command-config /etc/kafka/client.properties \
  --describe \
  --all-groups

# Check lag for a specific consumer group
kubectl exec -it -n logisticos-core kafka-client-pod -- \
  kafka-consumer-groups.sh \
  --bootstrap-server ${KAFKA_BOOTSTRAP} \
  --command-config /etc/kafka/client.properties \
  --describe \
  --group logisticos-dispatch-consumer

# List all topics
kubectl exec -it -n logisticos-core kafka-client-pod -- \
  kafka-topics.sh \
  --bootstrap-server ${KAFKA_BOOTSTRAP} \
  --command-config /etc/kafka/client.properties \
  --list

# Describe a topic (replication factor, ISR, leader)
kubectl exec -it -n logisticos-core kafka-client-pod -- \
  kafka-topics.sh \
  --bootstrap-server ${KAFKA_BOOTSTRAP} \
  --command-config /etc/kafka/client.properties \
  --describe \
  --topic logisticos.shipment.events
```

### Health and Metrics Endpoints

Every LogisticOS service exposes standard health endpoints:

```bash
# Port-forward to check service health directly
kubectl port-forward -n logisticos-logistics svc/dispatch 8080:8080

curl http://localhost:8080/health   # Liveness
curl http://localhost:8080/ready    # Readiness (includes dependency checks)
curl http://localhost:8080/metrics  # Prometheus metrics
```

---

## 6. Service-Specific Diagnosis

### Dispatch Service

```bash
# Check if VRP solver is timing out (look for vrp_optimization_duration > 2s)
curl -s http://localhost:8080/metrics | grep logisticos_vrp_optimization_duration

# Check driver assignment queue depth
curl -s http://localhost:8080/metrics | grep logisticos_assignment_queue_depth

# Review dispatch assignment errors in logs
kubectl logs -n logisticos-logistics -l app=dispatch --since=10m \
  | grep "assignment_failed\|vrp_error\|driver_unavailable"
```

### Identity / API Gateway

```bash
# Check JWT validation failure rate
kubectl logs -n logisticos-core -l app=api-gateway --since=5m \
  | grep "jwt_invalid\|auth_failed\|rate_limit_exceeded"

# Check Vault connectivity (secrets fetch failures)
kubectl logs -n logisticos-core -l app=identity --since=5m \
  | grep "vault_error\|secret_fetch_failed"
```

### Payments Service

```bash
# Check for stuck COD reconciliation jobs
kubectl logs -n logisticos-payments -l app=payments --since=10m \
  | grep "reconciliation_error\|cod_mismatch\|payment_gateway_timeout"

# Verify payment gateway connectivity
kubectl exec -it -n logisticos-payments \
  $(kubectl get pod -n logisticos-payments -l app=payments -o name | head -1) \
  -- curl -sf https://api.stripe.com/v1/balance -H "Authorization: Bearer $STRIPE_KEY_REDACTED"
```

### Engagement Engine

```bash
# Check WhatsApp / SMS delivery failures
kubectl logs -n logisticos-engagement -l app=engagement --since=10m \
  | grep "channel_error\|twilio_error\|whatsapp_failed"

# Check Redis pub/sub connectivity
kubectl logs -n logisticos-engagement -l app=engagement --since=5m \
  | grep "redis_pubsub_error\|subscription_lost"
```

### AI Layer

```bash
# Check Claude API failures
kubectl logs -n logisticos-ai -l app=ai-layer --since=10m \
  | grep "claude_api_error\|anthropic_rate_limit\|agent_timeout"

# Check MCP tool call failures
kubectl logs -n logisticos-ai -l app=ai-layer --since=10m \
  | grep "mcp_tool_error\|tool_call_failed"
```

---

## 7. Rollback Procedures

### Option A — Helm Rollback (Preferred)

```bash
# List recent Helm releases for a service to find the previous revision
helm history -n <namespace> <release-name>

# Example: rollback dispatch service to previous revision
helm history -n logisticos-logistics dispatch
# Note the REVISION number of the last known-good release

helm rollback -n logisticos-logistics dispatch <revision-number>

# Verify rollback completed
helm status -n logisticos-logistics dispatch
kubectl rollout status deployment/dispatch -n logisticos-logistics
```

### Option B — Istio Canary Reset

If a canary deployment is causing the incident (traffic split between stable and canary):

```bash
# Inspect current traffic split
kubectl get virtualservice dispatch -n logisticos-logistics -o yaml

# Reset to 100% stable — patch the VirtualService weight
kubectl patch virtualservice dispatch -n logisticos-logistics \
  --type='json' \
  -p='[
    {"op": "replace", "path": "/spec/http/0/route/0/weight", "value": 100},
    {"op": "replace", "path": "/spec/http/0/route/1/weight", "value": 0}
  ]'

# Confirm traffic is now 100% on stable
kubectl get virtualservice dispatch -n logisticos-logistics -o jsonpath='{.spec.http[0].route}'
```

### Option C — Kubernetes Deployment Rollback

```bash
# View rollout history
kubectl rollout history deployment/<deployment-name> -n <namespace>

# Roll back to the previous revision
kubectl rollout undo deployment/<deployment-name> -n <namespace>

# Roll back to a specific revision
kubectl rollout undo deployment/<deployment-name> -n <namespace> --to-revision=<N>

# Monitor rollout progress
kubectl rollout status deployment/<deployment-name> -n <namespace> --timeout=5m
```

### Post-Rollback Checks

After any rollback:

```bash
# 1. Verify pod restarts have settled
kubectl get pods -n <namespace> -l app=<service> -w

# 2. Hit the readiness endpoint
kubectl port-forward -n <namespace> svc/<service> 8080:8080
curl -sf http://localhost:8080/ready && echo "READY" || echo "NOT READY"

# 3. Check error rate in Grafana — confirm it is returning to baseline
# Dashboard: LogisticOS — Service Health → Error Rate panel

# 4. Check Kafka consumer lag — confirm queues are draining
kubectl exec -it -n logisticos-core kafka-client-pod -- \
  kafka-consumer-groups.sh \
  --bootstrap-server ${KAFKA_BOOTSTRAP} \
  --command-config /etc/kafka/client.properties \
  --describe \
  --group logisticos-<service>-consumer
```

---

## 8. Database Incident Procedure

### RDS Primary Failure / Automatic Failover

AWS RDS Multi-AZ performs automatic failover within 60–120 seconds. During failover:
- DNS endpoint updates to point to the promoted replica
- Active connections are dropped and must reconnect
- Services using SQLx connection pools will reconnect automatically

```bash
# Check RDS event log for failover events (AWS CLI)
aws rds describe-events \
  --source-identifier logisticos-production \
  --source-type db-instance \
  --duration 60 \
  --region ap-southeast-1

# Verify current writer endpoint
aws rds describe-db-instances \
  --db-instance-identifier logisticos-production \
  --query 'DBInstances[0].{Status:DBInstanceStatus,AZ:AvailabilityZone,Multi:MultiAZ}' \
  --region ap-southeast-1

# Check replica lag (if still replicating)
aws cloudwatch get-metric-statistics \
  --namespace AWS/RDS \
  --metric-name ReplicaLag \
  --dimensions Name=DBInstanceIdentifier,Value=logisticos-production \
  --start-time $(date -u -v-1H +%Y-%m-%dT%H:%M:%SZ) \
  --end-time $(date -u +%Y-%m-%dT%H:%M:%SZ) \
  --period 60 \
  --statistics Maximum \
  --region ap-southeast-1
```

### Long-Running Queries / Lock Contention

```bash
# Port-forward to RDS via a bastion pod running psql
kubectl exec -it -n logisticos-core db-admin-pod -- \
  psql "host=$RDS_HOST dbname=logisticos user=logisticos_admin sslmode=require"

-- Find queries running > 30 seconds
SELECT pid, now() - pg_stat_activity.query_start AS duration, query, state
FROM pg_stat_activity
WHERE state != 'idle'
  AND (now() - pg_stat_activity.query_start) > interval '30 seconds'
ORDER BY duration DESC;

-- Find blocking locks
SELECT
  blocked_locks.pid AS blocked_pid,
  blocked_activity.usename AS blocked_user,
  blocking_locks.pid AS blocking_pid,
  blocking_activity.usename AS blocking_user,
  blocked_activity.query AS blocked_statement,
  blocking_activity.query AS blocking_statement
FROM pg_catalog.pg_locks blocked_locks
JOIN pg_catalog.pg_stat_activity blocked_activity
  ON blocked_activity.pid = blocked_locks.pid
JOIN pg_catalog.pg_locks blocking_locks
  ON blocking_locks.locktype = blocked_locks.locktype
  AND blocking_locks.relation IS NOT DISTINCT FROM blocked_locks.relation
  AND blocking_locks.page IS NOT DISTINCT FROM blocked_locks.page
  AND blocking_locks.tuple IS NOT DISTINCT FROM blocked_locks.tuple
  AND blocking_locks.transactionid IS NOT DISTINCT FROM blocked_locks.transactionid
  AND blocking_locks.classid IS NOT DISTINCT FROM blocked_locks.classid
  AND blocking_locks.objid IS NOT DISTINCT FROM blocked_locks.objid
  AND blocking_locks.objsubid IS NOT DISTINCT FROM blocked_locks.objsubid
  AND blocking_locks.pid != blocked_locks.pid
JOIN pg_catalog.pg_stat_activity blocking_activity
  ON blocking_activity.pid = blocking_locks.pid
WHERE NOT blocked_locks.granted;

-- Terminate a blocking query (use with caution — get Engineering Manager approval)
-- SELECT pg_terminate_backend(<pid>);
```

### Disk Space Emergency

If RDS disk usage exceeds 85%, trigger storage autoscaling manually if not auto-triggered:

```bash
aws rds modify-db-instance \
  --db-instance-identifier logisticos-production \
  --allocated-storage <new-size-gib> \
  --apply-immediately \
  --region ap-southeast-1
```

Immediate mitigation: truncate or archive old audit log partitions (coordinate with DRE).

---

## 9. Kafka / MSK Incident Procedure

### Under-Replicated Partitions

Under-replicated partitions (URP > 0) indicate a broker failure or network partition. Durability is at risk.

```bash
# Check URP count via CloudWatch
aws cloudwatch get-metric-statistics \
  --namespace AWS/Kafka \
  --metric-name UnderReplicatedPartitions \
  --dimensions Name=Cluster\ Name,Value=logisticos-production \
  --start-time $(date -u -v-15M +%Y-%m-%dT%H:%M:%SZ) \
  --end-time $(date -u +%Y-%m-%dT%H:%M:%SZ) \
  --period 60 \
  --statistics Maximum \
  --region ap-southeast-1

# List topics with under-replicated partitions
kubectl exec -it -n logisticos-core kafka-client-pod -- \
  kafka-topics.sh \
  --bootstrap-server ${KAFKA_BOOTSTRAP} \
  --command-config /etc/kafka/client.properties \
  --describe \
  --under-replicated-partitions
```

**Resolution:** URPs typically self-heal when the affected broker recovers. If a broker is permanently lost, MSK will replace it. Do not manually reassign partitions unless directed by AWS Support.

### Consumer Lag Spike

A sudden consumer lag spike indicates a slow consumer, GC pause, or upstream burst.

```bash
# Identify which consumer group and topic have the lag
kubectl exec -it -n logisticos-core kafka-client-pod -- \
  kafka-consumer-groups.sh \
  --bootstrap-server ${KAFKA_BOOTSTRAP} \
  --command-config /etc/kafka/client.properties \
  --describe \
  --all-groups \
  | sort -k6 -rn \
  | head -20

# Scale up the consumer deployment if throughput is insufficient
kubectl scale deployment/<service>-consumer -n <namespace> --replicas=<N>

# Check if the consumer is in a crash loop
kubectl get pods -n <namespace> -l app=<service>-consumer
kubectl logs -n <namespace> -l app=<service>-consumer --since=5m | grep ERROR
```

---

## 10. Post-Incident Review Template

Create a PIR document in Confluence under `Engineering > PIRs > {Year}` within 24 hours of SEV1 resolution. Complete within 5 business days.

---

```
# Post-Incident Review — {Short Title}

**Incident ID:** INC-{YYYY-MM-DD}-{N}
**Severity:** SEV{1|2}
**Date/Time (SGT):** {start} → {end}
**Duration:** {X hours Y minutes}
**Services Affected:** {list}
**Tenants Impacted:** {count or "all"}
**Incident Commander:** {name}
**Scribe:** {name}
**Participants:** {names}

---

## Impact Summary

- **Customer impact:** {describe what customers experienced}
- **Revenue impact:** {estimated COD/transaction volume affected, if applicable}
- **SLA breach:** {yes/no — which tenants, which SLAs}

---

## Timeline

| Time (SGT) | Event |
|-----------|-------|
| HH:MM     | Alert fired in PagerDuty |
| HH:MM     | Acknowledged by {name} |
| HH:MM     | Incident channel created |
| HH:MM     | Root cause identified |
| HH:MM     | Mitigation applied |
| HH:MM     | Service restored |
| HH:MM     | Incident closed |

---

## Root Cause Analysis

**Root Cause (5 Whys):**

1. Why did the incident occur? {answer}
2. Why did that happen? {answer}
3. Why did that happen? {answer}
4. Why did that happen? {answer}
5. Why did that happen? {root cause}

**Contributing Factors:**
- {list any contributing conditions}

---

## Detection

- How was the incident detected? {PagerDuty alert / customer report / internal monitoring}
- Time from incident start to detection: {X minutes}
- Were existing monitors sufficient? {yes/no — explain}

---

## Response

- Was the runbook followed? {yes/no/partially}
- Were runbook gaps identified? {describe}
- Was escalation appropriate and timely? {yes/no}
- Was communication to stakeholders timely and accurate? {yes/no}

---

## Mitigation Applied

{Describe exactly what was done to restore service}

---

## Action Items

| Action | Owner | Priority | Due Date | Ticket |
|--------|-------|----------|----------|--------|
| {prevent recurrence} | {name} | P1 | {date} | {link} |
| {improve detection} | {name} | P2 | {date} | {link} |
| {update runbook} | {name} | P3 | {date} | {link} |
| {add test coverage} | {name} | P2 | {date} | {link} |

---

## What Went Well

- {list}

## What Could Be Improved

- {list}

---

**PIR Status:** Draft / In Review / Final
**Reviewed By:** {EM name, date}
```

---

## Appendix A — Useful Grafana Dashboards

| Dashboard | URL |
|-----------|-----|
| Service Health Overview | `https://grafana.logisticos.internal/d/logisticos-service-health` |
| Dispatch Operations | `https://grafana.logisticos.internal/d/logisticos-dispatch-ops` |
| AI Agents | `https://grafana.logisticos.internal/d/logisticos-ai-agents` |
| Infrastructure Overview | `https://grafana.logisticos.internal/d/logisticos-infra` |
| Kafka / MSK | `https://grafana.logisticos.internal/d/logisticos-kafka` |

## Appendix B — Escalation Contacts

| Role | PagerDuty Schedule | Slack Handle |
|------|--------------------|-------------|
| SRE Primary On-Call | `logisticos-sre-primary` | @oncall-sre |
| SRE Secondary On-Call | `logisticos-sre-secondary` | @oncall-sre-backup |
| Engineering Manager — Platform | — | @em-platform |
| CTO | — | @cto |
| CISO (security incidents only) | `logisticos-security` | @ciso |

## Appendix C — AWS Console Quick Links

- **EKS Clusters:** `https://ap-southeast-1.console.aws.amazon.com/eks/home?region=ap-southeast-1#/clusters`
- **RDS Instances:** `https://ap-southeast-1.console.aws.amazon.com/rds/home?region=ap-southeast-1#databases:`
- **MSK Clusters:** `https://ap-southeast-1.console.aws.amazon.com/msk/home?region=ap-southeast-1#/clusters`
- **CloudWatch Alarms:** `https://ap-southeast-1.console.aws.amazon.com/cloudwatch/home?region=ap-southeast-1#alarmsV2:`
- **ElastiCache:** `https://ap-southeast-1.console.aws.amazon.com/elasticache/home?region=ap-southeast-1#/redis`
