# Dispatch Service VPS Hotfix — April 28, 2026

## Issue
Dispatch service returning 500 errors: `no column found for name: dispatched_at`

## Root Cause
VPS database migrations 0001–0007 were not applied before the dispatch service started expecting the `dispatched_at` column.

## Immediate Fix (Manual SQL)

Run this SQL on the VPS svc_driver_ops database to add the missing column:

```sql
-- Connect to svc_driver_ops database
\c svc_driver_ops

-- Add the missing dispatched_at column to dispatch_queue
ALTER TABLE dispatch.dispatch_queue
    ADD COLUMN IF NOT EXISTS dispatched_at TIMESTAMPTZ;

-- Verify the column was added
SELECT column_name FROM information_schema.columns
WHERE table_name='dispatch_queue' AND column_name='dispatched_at'
ORDER BY ordinal_position;
```

### Via Docker on VPS:

```bash
# SSH into VPS
ssh root@os-api.cargomarket.net

# Run SQL via docker exec
docker exec logisticos-postgres psql -U logisticos -d svc_driver_ops << 'SQL'
ALTER TABLE dispatch.dispatch_queue
    ADD COLUMN IF NOT EXISTS dispatched_at TIMESTAMPTZ;

SELECT column_name FROM information_schema.columns
WHERE table_name='dispatch_queue' AND column_name='dispatched_at'
ORDER BY ordinal_position;
SQL

# Expected output: should show "dispatched_at" in the column list
```

## Permanent Fix (Code Deployment)

A new safeguard migration (0008) has been added to the codebase:
- File: `services/dispatch/migrations/0008_ensure_dispatched_at_exists.sql`
- This will automatically add the column on next deployment

Steps:
1. Redeploy the dispatch service via Dokploy
2. The migration will run automatically during service startup
3. Column will be created (safely, with IF NOT EXISTS)
4. Dispatch service will resume normal operation

## Verification

After applying the fix, test the dispatch flow:

1. Create a shipment via Merchant Portal
2. Admin Portal → Dispatch Console → should see the shipment
3. Click "Dispatch" button
4. POST /v1/queue/{shipmentId}/dispatch should return 200 (no more 500)
5. Driver App should receive the task

## Related Issues

### Secondary Issue: Orphan Cleanup Warning
The recurring warning about `make_interval` is from the orphan-assignment cleanup job (bootstrap.rs:158-184). This is non-critical and should resolve once the dispatch_queue column is added.

## Timeline
- Issue discovered: 2026-04-28 13:45 UTC
- Hotfix created: 2026-04-28 14:00 UTC
- Manual SQL provided: Above
- Code fix committed: commit `68d5129`
- Ready for redeployment: Immediately
