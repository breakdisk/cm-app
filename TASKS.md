# LogisticOS — Active Tasks

## Session: 2026-06-14 — Gig Worker Onboarding + Compliance + Merchant Setup

### Merchant Onboarding Finalization
- [x] 1. Add `finalizeTenant` + `FinalizeTenantPayload` + `status` field to `identityApi`
- [x] 2. Create merchant portal `/setup` page (business name + currency + region wizard)
- [x] 3. Add draft-tenant redirect guard in dashboard layout.tsx

### Gig Worker Onboarding (Admin Portal)
- [x] 4. Add "Gig Worker" option to OnboardDriverModal (clarify part_time = gig pool)
- [x] 5. Add compliance reminder on OnboardDriverModal success screen

### Compliance Visibility (Admin Portal)
- [x] 6. Add compliance status badge to driver cards in drivers page

---
### Also In Progress (uncommitted diff — commit after this session)
- [x] identity: migration 0015 + tenant entity/repo/service currency+region fields
- [x] events: TenantFinalized carries currency+region
- [x] engagement: TENANT_FINALIZED → merchant_welcome email
- [x] payments: COD balance pending_count/shipments_pending_cod
- [x] merchant portal billing: Download PDF + Resend buttons, COD balance fix
- [x] merchant portal identity.ts: Tenant type + getTenant + finalizeTenant
