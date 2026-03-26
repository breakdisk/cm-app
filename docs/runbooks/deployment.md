# Deployment Runbook

**Owner:** Staff Platform Engineer / SRE Lead
**Last Reviewed:** 2026-03-17
**Applies To:** All LogisticOS production and staging environments
**Related ADRs:** ADR-0001 (Zero-downtime deployments), ADR-0002 (Kubernetes architecture)

---

## Table of Contents

1. [Pre-Deployment Checklist](#1-pre-deployment-checklist)
2. [Standard Deployment via GitHub Actions](#2-standard-deployment-via-github-actions)
3. [Manual Helm Deploy Commands](#3-manual-helm-deploy-commands)
4. [Canary Traffic Shifting Procedure](#4-canary-traffic-shifting-procedure)
5. [Database Migration Procedure](#5-database-migration-procedure)
6. [Rollback Procedure](#6-rollback-procedure)
7. [Post-Deploy Smoke Tests](#7-post-deploy-smoke-tests)
8. [Kafka Consumer Lag Check](#8-kafka-consumer-lag-check)
9. [Grafana Dashboards to Monitor During Deploy](#9-grafana-dashboards-to-monitor-during-deploy)

---

## 1. Pre-Deployment Checklist

Complete all items before initiating a production deployment. Staging deployments require only the items marked **[staging]**.

### Change Window

- [ ] Production deployments must occur within a defined change window: **Tuesday, Wednesday, or Thursday, 10:00–14:00 SGT**.
- [ ] Emergency hotfixes outside the change window require Engineering Manager + SRE Lead approval, documented in Slack `#deployments`.
- [ ] Do not deploy within 48 hours of a major commercial event (e.g., Harbolnas, 12.12, Valentine's Day) unless it is a critical hotfix.

### Stakeholder Notification

```
# Post in #deployments channel at least 30 minutes before production deploy:
"🚀 Deploying <service-name> v<version> to production in ~30min.
 Changes: <link to PR/changelog>
 Rollback plan: Helm rollback to revision <N> (see Section 6)
 On-call contact: @{your-handle}"
```

For deployments affecting the Driver App backend, Payments, or Engagement Engine,
additionally notify the relevant product manager.

### Technical Readiness Gates

- [ ] PR merged to `main` with minimum 2 approvals (one senior engineer or architect)
- [ ] All CI gates green: `clippy`, `cargo test`, `cargo audit`, security scan, OpenAPI lint
- [ ] Docker image tagged with Git SHA and pushed to ECR
- [ ] Staging deployment completed and smoke-tested (see Section 7)
- [ ] E2E tests passed in staging environment (Playwright suite)
- [ ] Performance regression check passed (P99 latency within 10% of baseline)
- [ ] Database migrations reviewed (if any) — see Section 5
- [ ] Rollback plan documented: target Helm revision or Docker image SHA

### For Database Migrations

- [ ] Migration is reviewed by a senior engineer or DRE (Database Reliability Engineer)
- [ ] Migration is backward-compatible with the **previous** service version (for safe rollback)
- [ ] Migration is tested in staging with production-volume data clone
- [ ] Estimated migration duration noted (if > 60s, plan for maintenance window)
- [ ] Migration is idempotent (`IF NOT EXISTS`, `ON CONFLICT DO NOTHING`)

---

## 2. Standard Deployment via GitHub Actions

The preferred deployment path is the `deploy.yml` GitHub Actions workflow. It runs
CI, builds and pushes the Docker image, deploys to staging, runs E2E tests, then
promotes to production via a canary rollout.

### Trigger

```bash
# The workflow is triggered automatically on merge to main.
# For a manual deploy of a specific SHA or service:
gh workflow run deploy.yml \
  -f service=dispatch \
  -f environment=production \
  -f image_tag=sha-abc1234
```

### Workflow Stages

```
[CI] cargo check → clippy → cargo test → cargo audit
        ↓
[Build] docker build + push to ECR (tagged :sha-{git-sha} and :{version})
        ↓
[Deploy Staging] helm upgrade --install → wait for rollout → smoke test
        ↓
[E2E Tests] Playwright suite against staging
        ↓ (requires manual approval for production in GitHub Actions)
[Deploy Production — Canary 10%] helm upgrade with canary values
        ↓
[Smoke Test Production] curl /health, /ready, /metrics
        ↓ (auto-promote if error rate < 1% after 10 min, else alert on-call)
[Promote to 100%] VirtualService weight update via kubectl patch
        ↓
[Post-Deploy Validation] Kafka lag check + Grafana error rate baseline
```

### Monitor the Workflow

```bash
# Watch the live workflow run
gh run watch

# List recent runs for a service
gh run list --workflow=deploy.yml --limit=10

# View logs for a failed step
gh run view <run-id> --log-failed
```

---

## 3. Manual Helm Deploy Commands

Use manual Helm commands only when the GitHub Actions workflow is unavailable or
when deploying a hotfix that bypasses the standard promotion flow.

### Namespace Reference

| Service | Namespace | Helm Release Name |
|---------|-----------|------------------|
| identity | logisticos-core | identity |
| api-gateway | logisticos-core | api-gateway |
| business-logic | logisticos-core | business-logic |
| order-intake | logisticos-logistics | order-intake |
| dispatch | logisticos-logistics | dispatch |
| driver-ops | logisticos-logistics | driver-ops |
| fleet | logisticos-logistics | fleet |
| hub-ops | logisticos-logistics | hub-ops |
| carrier | logisticos-logistics | carrier |
| pod | logisticos-logistics | pod |
| engagement | logisticos-engagement | engagement |
| cdp | logisticos-engagement | cdp |
| marketing | logisticos-engagement | marketing |
| payments | logisticos-payments | payments |
| ai-layer | logisticos-ai | ai-layer |
| analytics | logisticos-analytics | analytics |

### Deploy a Service

```bash
# Set context to production cluster
kubectl config use-context logisticos-production

# Deploy with a specific image tag (replace <service>, <namespace>, <image-tag>)
helm upgrade --install <service> ./infra/kubernetes/charts/<service> \
  --namespace <namespace> \
  --values ./infra/kubernetes/charts/<service>/values.yaml \
  --values ./infra/kubernetes/charts/<service>/values.production.yaml \
  --set image.tag=<image-tag> \
  --set deploy.timestamp="$(date -u +%Y%m%dT%H%M%SZ)" \
  --wait \
  --timeout 10m

# Verify the deployment rolled out successfully
kubectl rollout status deployment/<service> -n <namespace> --timeout=5m

# Confirm the new pod version
kubectl get pods -n <namespace> -l app=<service> -o wide
```

### Examples

```bash
# Deploy dispatch service with image sha-abc1234
helm upgrade --install dispatch ./infra/kubernetes/charts/dispatch \
  --namespace logisticos-logistics \
  --values ./infra/kubernetes/charts/dispatch/values.yaml \
  --values ./infra/kubernetes/charts/dispatch/values.production.yaml \
  --set image.tag=sha-abc1234 \
  --wait --timeout 10m

# Deploy payments service
helm upgrade --install payments ./infra/kubernetes/charts/payments \
  --namespace logisticos-payments \
  --values ./infra/kubernetes/charts/payments/values.yaml \
  --values ./infra/kubernetes/charts/payments/values.production.yaml \
  --set image.tag=sha-abc1234 \
  --wait --timeout 10m
```

---

## 4. Canary Traffic Shifting Procedure

All production deployments use Istio VirtualService traffic splitting for canary rollouts.
The canary deployment (`<service>-canary`) runs alongside the stable deployment.

### Initial State (before deploy)

```
Stable: 100%   Canary: 0%
```

### Step 1 — Deploy Canary Workload (0% traffic)

```bash
# Deploy the canary Helm release with 0% traffic weight
helm upgrade --install <service>-canary ./infra/kubernetes/charts/<service> \
  --namespace <namespace> \
  --values ./infra/kubernetes/charts/<service>/values.yaml \
  --values ./infra/kubernetes/charts/<service>/values.production.yaml \
  --values ./infra/kubernetes/charts/<service>/values.canary.yaml \
  --set image.tag=<new-image-tag> \
  --wait --timeout 10m
```

### Step 2 — Shift 10% of Traffic to Canary

```bash
kubectl patch virtualservice <service> -n <namespace> \
  --type='json' \
  -p='[
    {"op": "replace", "path": "/spec/http/0/route/0/weight", "value": 90},
    {"op": "replace", "path": "/spec/http/0/route/1/weight", "value": 10}
  ]'
```

**Wait 10 minutes.** Monitor Grafana error rate and P99 latency for the canary subset
(filter by `destination_version="canary"` in the Service Health dashboard).
Proceed only if canary error rate is < 1% and latency is within 10% of stable baseline.

### Step 3 — Shift 50% of Traffic to Canary

```bash
kubectl patch virtualservice <service> -n <namespace> \
  --type='json' \
  -p='[
    {"op": "replace", "path": "/spec/http/0/route/0/weight", "value": 50},
    {"op": "replace", "path": "/spec/http/0/route/1/weight", "value": 50}
  ]'
```

**Wait 10 minutes.** Same validation criteria. Abort to stable if any anomaly is detected.

### Step 4 — Promote to 100% Canary

```bash
kubectl patch virtualservice <service> -n <namespace> \
  --type='json' \
  -p='[
    {"op": "replace", "path": "/spec/http/0/route/0/weight", "value": 0},
    {"op": "replace", "path": "/spec/http/0/route/1/weight", "value": 100}
  ]'
```

### Step 5 — Retire Stable Workload

After 30 minutes of stable operation at 100% canary:

```bash
# Promote canary to become the new stable
helm upgrade --install <service> ./infra/kubernetes/charts/<service> \
  --namespace <namespace> \
  --values ./infra/kubernetes/charts/<service>/values.yaml \
  --values ./infra/kubernetes/charts/<service>/values.production.yaml \
  --set image.tag=<new-image-tag> \
  --wait --timeout 10m

# Remove the canary release
helm uninstall <service>-canary -n <namespace>

# Reset VirtualService to 100% stable (single destination)
kubectl patch virtualservice <service> -n <namespace> \
  --type='json' \
  -p='[
    {"op": "replace", "path": "/spec/http/0/route/0/weight", "value": 100}
  ]'
```

### Verify Final State

```bash
kubectl get virtualservice <service> -n <namespace> \
  -o jsonpath='{.spec.http[0].route}' | python3 -m json.tool
```

---

## 5. Database Migration Procedure

Migrations must be run **before** the new service version is deployed. The new
service version must be backward-compatible with the pre-migration schema (so that
the current stable can still run while migrations are applied).

### Pre-Migration Validation

```bash
# 1. Confirm you are connected to the correct database
kubectl exec -it -n logisticos-core db-admin-pod -- \
  psql "$DATABASE_URL" -c "SELECT current_database(), current_user, version();"

# 2. Check migration state — list already-applied migrations
# (LogisticOS uses sqlx migrations; the _sqlx_migrations table is authoritative)
kubectl exec -it -n logisticos-core db-admin-pod -- \
  psql "$DATABASE_URL" -c "SELECT version, description, installed_on FROM _sqlx_migrations ORDER BY installed_on DESC LIMIT 10;"

# 3. Dry-run: review the migration SQL before executing
cat services/<service>/migrations/<migration-file>.sql
```

### Apply Migration

```bash
# Run migrations via sqlx CLI from inside a migration job pod
kubectl exec -it -n logisticos-core db-admin-pod -- \
  sqlx migrate run \
  --source /migrations/<service> \
  --database-url "$DATABASE_URL"

# For large tables (millions of rows): run with LOCK_TIMEOUT to detect contention
kubectl exec -it -n logisticos-core db-admin-pod -- \
  psql "$DATABASE_URL" \
  -c "SET lock_timeout = '5s';" \
  -f /migrations/<service>/<migration-file>.sql
```

### Verify Migration Applied

```bash
kubectl exec -it -n logisticos-core db-admin-pod -- \
  psql "$DATABASE_URL" \
  -c "SELECT version, description, installed_on, success FROM _sqlx_migrations ORDER BY installed_on DESC LIMIT 5;"

# Confirm the new table or column exists
kubectl exec -it -n logisticos-core db-admin-pod -- \
  psql "$DATABASE_URL" \
  -c "\d <schema>.<table>"
```

### Zero-Downtime Migration Rules

All migrations must be non-destructive and backward-compatible. Follow these rules:

| Operation | Approach |
|-----------|---------|
| Add column | `ADD COLUMN ... DEFAULT NULL` or `ADD COLUMN ... DEFAULT <value>` — always safe |
| Drop column | Two-step: (1) stop writing to it in app code, deploy; (2) migration to drop column, deploy |
| Rename column | Two-step: (1) add new column + backfill, deploy dual-write; (2) drop old column |
| Add index | `CREATE INDEX CONCURRENTLY` to avoid table lock |
| Add NOT NULL constraint | Requires backfill first; add as NULLABLE, backfill, then add constraint |
| Rename table | Two-step: add view with old name, migrate app, then drop view |

```bash
# ALWAYS use CONCURRENTLY for new indexes on large tables
psql "$DATABASE_URL" -c "CREATE INDEX CONCURRENTLY IF NOT EXISTS ..."

# NEVER use plain CREATE INDEX on a production table with > 100k rows
# It holds a ShareLock for the full index build duration
```

---

## 6. Rollback Procedure

If a deployment causes errors, follow this procedure immediately. Do not wait to
diagnose the root cause before rolling back — roll back first, investigate second.

### Option A — Helm Rollback (Preferred)

```bash
# List recent Helm revisions to identify the last known-good version
helm history -n <namespace> <service>

# Rollback to the previous revision
helm rollback -n <namespace> <service> <revision-number>

# Verify rollback completed
helm status -n <namespace> <service>
kubectl rollout status deployment/<service> -n <namespace>
```

### Option B — Istio Canary Emergency Reset

If a canary deployment is sending bad traffic:

```bash
# Immediately reset all traffic to the stable version
kubectl patch virtualservice <service> -n <namespace> \
  --type='json' \
  -p='[
    {"op": "replace", "path": "/spec/http/0/route/0/weight", "value": 100},
    {"op": "replace", "path": "/spec/http/0/route/1/weight", "value": 0}
  ]'

# Confirm traffic is 100% on stable
kubectl get virtualservice <service> -n <namespace> \
  -o jsonpath='{.spec.http[0].route}' | python3 -m json.tool
```

### Post-Rollback: Database Migration Rollback

Most migrations cannot be automatically reversed. If the deployment requires a migration
rollback, coordinate with the DRE:

```bash
# Check if the migration has a down migration file
ls services/<service>/migrations/

# Apply the down migration manually if available
kubectl exec -it -n logisticos-core db-admin-pod -- \
  psql "$DATABASE_URL" -f /migrations/<service>/<down-migration>.sql

# If no down migration: apply a compensating migration that reverses the schema change
# This must be reviewed and approved before execution
```

---

## 7. Post-Deploy Smoke Tests

Run these checks immediately after every deployment (staging and production).

### Health and Readiness

```bash
# Port-forward to the deployed service (replace <service> and <namespace>)
kubectl port-forward -n <namespace> svc/<service> 8080:8080 &

# Liveness check — service must return 200 OK
curl -sf http://localhost:8080/health && echo "PASS: health" || echo "FAIL: health"

# Readiness check — all dependencies (DB, Redis, Kafka) must be reachable
curl -sf http://localhost:8080/ready  && echo "PASS: ready"  || echo "FAIL: ready"

# Metrics endpoint — must return Prometheus text format
curl -sf http://localhost:8080/metrics | head -20

# Kill the port-forward when done
kill %1
```

### Smoke Tests by Service

```bash
# ── Identity ──
# Validate token endpoint is responding
kubectl port-forward -n logisticos-core svc/identity 8080:8080 &
curl -sf -X POST http://localhost:8080/health && echo "PASS: identity health"
kill %1

# ── Dispatch ──
# Check VRP optimizer is loaded (metric must be present)
kubectl port-forward -n logisticos-logistics svc/dispatch 8080:8080 &
curl -sf http://localhost:8080/metrics | grep -q "logisticos_vrp" && echo "PASS: vrp metric present"
kill %1

# ── Payments ──
# Confirm payment gateway connectivity check passes in readiness
kubectl port-forward -n logisticos-payments svc/payments 8080:8080 &
curl -sf http://localhost:8080/ready | python3 -c "import sys,json; d=json.load(sys.stdin); assert d['dependencies']['payment_gateway']=='ok', 'FAIL'" && echo "PASS: payments gateway"
kill %1

# ── Engagement ──
# Confirm Redis pub/sub connection is live
kubectl port-forward -n logisticos-engagement svc/engagement 8080:8080 &
curl -sf http://localhost:8080/ready | python3 -c "import sys,json; d=json.load(sys.stdin); assert d['dependencies']['redis']=='ok', 'FAIL'" && echo "PASS: engagement redis"
kill %1
```

### API Gateway Route Smoke Test

```bash
# Test the API gateway is routing to the newly deployed service
# (Requires a valid staging/production API key in LOGISTICOS_API_KEY env var)

# Health route (no auth required)
curl -sf https://api.logisticos.com/health && echo "PASS: gateway health"

# Auth-gated route (expect 200 or 401, not 502/503)
STATUS=$(curl -s -o /dev/null -w "%{http_code}" \
  https://api.logisticos.com/v1/shipments \
  -H "Authorization: Bearer $LOGISTICOS_API_KEY")
echo "Gateway route status: $STATUS"
[ "$STATUS" == "200" ] || [ "$STATUS" == "401" ] && echo "PASS" || echo "FAIL: 5xx from gateway"
```

---

## 8. Kafka Consumer Lag Check Post-Deploy

Consumer lag is expected to be near-zero during steady state. A lag spike immediately
after deploy indicates the new version's consumers are not processing messages or
crashed during startup.

```bash
# Check lag for all consumer groups owned by the deployed service
kubectl exec -it -n logisticos-core kafka-client-pod -- \
  kafka-consumer-groups.sh \
  --bootstrap-server ${KAFKA_BOOTSTRAP} \
  --command-config /etc/kafka/client.properties \
  --describe \
  --group logisticos-<service>-consumer

# Expected output: LAG column should be < 100 within 2 minutes of deploy
# Consumer groups by service:
#   dispatch:   logisticos-dispatch-consumer
#   engagement: logisticos-engagement-consumer
#   cdp:        logisticos-cdp-consumer
#   payments:   logisticos-payments-consumer
#   analytics:  logisticos-analytics-consumer
#   marketing:  logisticos-marketing-consumer
```

### Interpreting Lag

| LAG | Interpretation | Action |
|-----|---------------|--------|
| 0–100 | Normal — consumers keeping up | None |
| 100–1,000 | Minor transient lag — normal post-restart catch-up | Monitor; should drain within 5 minutes |
| 1,000–10,000 | Elevated — consumer may be slow or partially crashed | Check consumer pod logs; scale up if needed |
| > 10,000 | Critical — consumers not processing; escalate | Consider rollback; check for crash loops |

```bash
# If lag is elevated: check for crash loops in the consumer pods
kubectl get pods -n <namespace> -l app=<service> | grep -v Running

# Scale up consumers temporarily if lag is due to high post-deploy burst
kubectl scale deployment/<service> -n <namespace> --replicas=<N+2>
```

---

## 9. Grafana Dashboards to Monitor During Deploy

Keep these dashboards open during and after every production deployment.
Give each at least 10–15 minutes of observation time after the deploy completes.

| Dashboard | URL | What to Watch |
|-----------|-----|---------------|
| **Service Health Overview** | `https://grafana.logisticos.internal/d/logisticos-service-health` | Error rate, P99 latency, request rate for deployed service |
| **Dispatch Operations** | `https://grafana.logisticos.internal/d/logisticos-dispatch-ops` | Assignment success rate, VRP latency, driver availability (deploy: dispatch, driver-ops) |
| **Payments & COD** | `https://grafana.logisticos.internal/d/logisticos-payments` | COD collection success, invoice generation, payment gateway error rate (deploy: payments) |
| **Engagement Delivery** | `https://grafana.logisticos.internal/d/logisticos-engagement` | WhatsApp/SMS delivery rate, notification queue depth (deploy: engagement, cdp, marketing) |
| **Kafka / MSK** | `https://grafana.logisticos.internal/d/logisticos-kafka` | Consumer group lag for affected service topics |
| **Infrastructure Overview** | `https://grafana.logisticos.internal/d/logisticos-infra` | Pod CPU/memory, node pressure, RDS connection count |
| **API Gateway** | `https://grafana.logisticos.internal/d/logisticos-api-gateway` | Gateway 5xx rate, route error rate, auth failure rate (all deploys) |

### Key Thresholds — Abort Criteria

If any of the following are observed during a canary rollout, abort immediately
(reset to 100% stable) and page the on-call SRE:

| Metric | Abort Threshold |
|--------|----------------|
| Service error rate (5xx) | > 1% for > 2 minutes |
| P99 API latency | > 200ms for > 5 minutes |
| P99 dispatch assignment | > 500ms for > 2 minutes |
| Kafka consumer lag | > 10,000 messages for > 5 minutes |
| Pod crash/restart count | Any CrashLoopBackOff in canary pods |
| RDS connection count | > 80% of max_connections |
