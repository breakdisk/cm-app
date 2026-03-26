# Driver Compliance System — Design Spec
**Date:** 2026-03-25
**Status:** Approved for implementation
**Author:** Principal Software Architect

---

## 1. Overview

LogisticOS currently has no structured driver compliance infrastructure. Drivers can be assigned tasks regardless of whether their licenses, vehicle registrations, or insurance documents are valid. This creates direct legal liability for tenant companies operating under UAE (RTA) and Philippines (LTO/LTFRB) regulations.

This spec defines a **generic `compliance` service** that manages document verification and compliance status for any entity type. The first implementation covers **driver compliance**. Partner and merchant compliance are follow-on specs that slot into the same service without schema changes.

### Goals

- Verify driver identity, license, vehicle, and insurance documents before allowing task assignment
- Support UAE and Philippines jurisdictions from day one via configurable document type registry
- Enforce a grace-period-based expiry model: warn → expire → suspend
- Give the platform admin team a review console to approve or reject submitted documents
- Give drivers a self-service document upload flow in the Driver App
- Block suspended drivers from dispatch automatically via Kafka events

### Out of Scope (This Spec)

- Partner / carrier compliance (follow-on spec)
- Merchant compliance (follow-on spec)
- Automated OCR document verification (future enhancement — manual review only for now)
- Cross-tenant compliance data sharing

---

## 2. Architecture

### New Service: `services/compliance/`

A standalone Rust/Axum microservice following the existing service pattern.

```
services/compliance/
├── src/
│   ├── domain/
│   │   ├── entities/          # ComplianceProfile, DriverDocument, DocumentType, AuditLog
│   │   ├── value_objects/     # ComplianceStatus, EntityType, DocumentStatus
│   │   └── events/            # ComplianceEvent enum (Kafka payloads)
│   ├── application/
│   │   ├── services/          # ComplianceService, ExpiryCheckerService
│   │   └── commands/          # SubmitDocument, ReviewDocument, SuspendEntity
│   ├── infrastructure/
│   │   ├── db/                # SQLx repositories
│   │   ├── kafka/             # Producer + consumer
│   │   └── storage/           # File upload (S3-compatible)
│   └── api/
│       ├── http/              # Axum routes
│       └── grpc/              # Tonic service impl
├── migrations/
└── Cargo.toml
```

### Integration Points

| Dependency | Direction | Purpose |
|---|---|---|
| `driver-ops` | Inbound Kafka | Listens to `driver.registered` → creates ComplianceProfile |
| `dispatch` | Outbound Kafka | Publishes `compliance.status_changed` → dispatch caches assignability |
| `engagement` | Outbound Kafka | Publishes expiry warnings and review outcomes → push/SMS notifications |
| `identity` | Inbound HTTP | Validates admin reviewer JWT claims |
| Object Storage (S3) | Outbound | Document image upload / retrieval |

---

## 3. Data Model

### 3.1 `compliance_profiles`

One record per entity (driver, partner, merchant). Overall compliance status is **derived** from required document states and recomputed on every document state change — never updated manually.

```sql
CREATE TABLE compliance_profiles (
  id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id        UUID NOT NULL,
  entity_type      TEXT NOT NULL CHECK (entity_type IN ('driver', 'partner', 'merchant')),
  entity_id        UUID NOT NULL,
  overall_status   TEXT NOT NULL DEFAULT 'pending_submission',
  jurisdiction     TEXT NOT NULL,           -- 'UAE', 'PH', 'GLOBAL'
  last_reviewed_at TIMESTAMPTZ,
  reviewed_by      UUID,                    -- admin user id
  suspended_at     TIMESTAMPTZ,
  created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at       TIMESTAMPTZ NOT NULL DEFAULT now(),

  UNIQUE (tenant_id, entity_type, entity_id)
);

CREATE INDEX idx_compliance_profiles_status   ON compliance_profiles (tenant_id, overall_status);
CREATE INDEX idx_compliance_profiles_entity   ON compliance_profiles (entity_type, entity_id);
```

**`overall_status` values:**

| Status | Meaning |
|---|---|
| `pending_submission` | One or more required docs not yet submitted |
| `under_review` | All required docs submitted; awaiting admin approval |
| `compliant` | All required docs approved and not yet expiring |
| `expiring_soon` | One or more docs expiring within `warn_days_before` threshold |
| `expired` | One or more docs past expiry; within grace period |
| `suspended` | Grace period elapsed or admin override |
| `rejected` | Admin rejected a required doc; driver must resubmit |

---

### 3.2 `document_types`

Seeded configuration registry. No code changes required to add a new jurisdiction or document type.

```sql
CREATE TABLE document_types (
  id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  code             TEXT NOT NULL UNIQUE,         -- e.g. 'UAE_DRIVING_LICENSE'
  jurisdiction     TEXT NOT NULL,                -- 'UAE', 'PH', 'GLOBAL'
  applicable_to    TEXT[] NOT NULL,              -- ['driver'], ['partner'], ['merchant']
  name             TEXT NOT NULL,
  description      TEXT,
  is_required      BOOLEAN NOT NULL DEFAULT true,
  has_expiry       BOOLEAN NOT NULL DEFAULT true,
  warn_days_before INT NOT NULL DEFAULT 30,      -- amber warning window
  grace_period_days INT NOT NULL DEFAULT 7,      -- days after expiry before suspension
  vehicle_classes  TEXT[],                       -- null = all classes; ['sedan','van'] = restricted
  created_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

**Seeded document types for this spec (driver):**

| Code | Jurisdiction | Expiry | Warn | Grace | Vehicle Classes |
|---|---|---|---|---|---|
| `UAE_EMIRATES_ID` | UAE | Yes | 60d | 14d | All |
| `UAE_DRIVING_LICENSE` | UAE | Yes | 30d | 7d | All |
| `UAE_VEHICLE_MULKIYA` | UAE | Yes | 30d | 7d | All |
| `UAE_VEHICLE_INSURANCE` | UAE | Yes | 30d | 7d | All |
| `PH_LTO_LICENSE` | PH | Yes | 30d | 7d | All |
| `PH_OR_CR` | PH | Yes | 30d | 7d | All |
| `PH_NBI_CLEARANCE` | PH | Yes | 60d | 14d | All |
| `PH_CTPL_INSURANCE` | PH | Yes | 30d | 7d | All |

*Follow-on specs will add `UAE_TRADE_LICENSE`, `PH_LTFRB_FRANCHISE`, `PH_TIN`, etc. with `applicable_to: ['partner']` or `['merchant']`.*

---

### 3.3 `driver_documents`

Each row is one document submission. When a driver renews a document, the old row is marked `superseded` and a new row is inserted.

```sql
CREATE TABLE driver_documents (
  id                    UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  compliance_profile_id UUID NOT NULL REFERENCES compliance_profiles(id),
  document_type_id      UUID NOT NULL REFERENCES document_types(id),
  document_number       TEXT NOT NULL,
  issue_date            DATE,
  expiry_date           DATE,
  file_url              TEXT NOT NULL,        -- S3 presigned path
  status                TEXT NOT NULL DEFAULT 'submitted',
  rejection_reason      TEXT,
  reviewed_by           UUID,
  reviewed_at           TIMESTAMPTZ,
  submitted_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at            TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_driver_documents_profile ON driver_documents (compliance_profile_id);
CREATE INDEX idx_driver_documents_expiry  ON driver_documents (expiry_date) WHERE status = 'approved';
```

**`status` values:** `submitted` → `under_review` → `approved` | `rejected` | `expired` | `superseded`

---

### 3.4 `compliance_audit_log`

Append-only. Never updated or deleted.

```sql
CREATE TABLE compliance_audit_log (
  id                    UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  compliance_profile_id UUID NOT NULL REFERENCES compliance_profiles(id),
  document_id           UUID REFERENCES driver_documents(id),
  event_type            TEXT NOT NULL,
  actor_id              UUID NOT NULL,
  actor_type            TEXT NOT NULL CHECK (actor_type IN ('driver', 'admin', 'system')),
  notes                 TEXT,
  created_at            TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_audit_log_profile ON compliance_audit_log (compliance_profile_id, created_at DESC);
```

**`event_type` values:** `doc_submitted`, `doc_approved`, `doc_rejected`, `expiry_warned`, `grace_started`, `driver_suspended`, `driver_reinstated`, `admin_override`

---

## 4. Workflow

### 4.1 Initial Onboarding

1. Driver registers → `driver-ops` publishes `driver.registered`
2. Compliance service consumes event → creates `ComplianceProfile` with `overall_status: pending_submission`
3. Driver App shows "Documents Required" checklist based on tenant jurisdiction
4. Driver photographs each document, enters document number and expiry date, submits
5. Service creates `DriverDocument` record, recomputes `overall_status` → `under_review`
6. Admin Portal review queue updates; admin reviews each document image + metadata
7. Admin approves or rejects with reason; service writes AuditLog, recomputes status
8. When all required docs approved → `overall_status: compliant`; publishes `compliance.driver_verified`
9. Dispatch service caches status → driver becomes assignable

### 4.2 Expiry and Renewal

1. Daily expiry checker scans `driver_documents` where `status = 'approved'` and `expiry_date IS NOT NULL`
2. For each document:
   - `expiry_date - today ≤ warn_days_before` → update `overall_status: expiring_soon`, publish `compliance.expiry_warning`
   - `expiry_date < today` and within `grace_period_days` → set `driver_documents.status: expired`, update `overall_status: expired`, publish `compliance.grace_started`
   - `expiry_date + grace_period_days < today` → update `overall_status: suspended`, publish `compliance.driver_suspended`
3. Engagement service sends push notification to driver and email alert to ops team on each event
4. Driver re-uploads renewed document from Driver App
5. Old document → `superseded`; new document → `submitted` → `under_review`
6. Admin approves renewal → `overall_status: compliant`; publishes `compliance.driver_reinstated`

### 4.3 Status State Machine

```
pending_submission
  → under_review          (driver submits all required docs)

under_review
  → compliant             (admin approves all required docs)
  → pending_submission    (admin rejects any required doc)

compliant
  → expiring_soon         (system: expiry_date ≤ warn_days_before)

expiring_soon
  → compliant             (renewal approved before expiry date)
  → expired               (expiry_date passed)

expired
  → compliant             (renewal approved within grace period)
  → suspended             (grace_period_days elapsed)

suspended
  → compliant             (renewal approved + admin clears suspension)

any → suspended           (admin_override)
```

### 4.4 Dispatch Gate

The dispatch service maintains a Redis cache of `driver_id → compliance_status`, updated on every `compliance.status_changed` Kafka event.

| Status | Assignable | Dispatcher View |
|---|---|---|
| `compliant` | ✓ Yes | Green indicator |
| `expiring_soon` | ✓ Yes | Amber warning badge |
| `expired` (within grace) | ✓ Yes (hardcoded default; dispatcher sees amber flag and may choose not to assign) | Amber flag, human decision |
| `pending_submission` | ✗ No | Blocked |
| `under_review` | ✗ No | Blocked |
| `rejected` | ✗ No | Blocked |
| `suspended` | ✗ No | Blocked |

---

## 5. Kafka Events

All events published to the `compliance` topic with tenant-scoped keys.

| Event | Trigger | Consumers |
|---|---|---|
| `compliance.profile_created` | Driver registered | — |
| `compliance.doc_submitted` | Driver submits document | Engagement (ACK notification) |
| `compliance.doc_approved` | Admin approves | Engagement (push to driver) |
| `compliance.doc_rejected` | Admin rejects | Engagement (push with reason) |
| `compliance.driver_verified` | All docs approved | Dispatch (add to pool), Engagement |
| `compliance.expiry_warning` | Expiry checker: warn window | Engagement (renewal reminder) |
| `compliance.grace_started` | Expiry date passed | Dispatch (flag), Engagement |
| `compliance.driver_suspended` | Grace elapsed / admin override | Dispatch (remove from pool), Engagement |
| `compliance.driver_reinstated` | Renewal approved | Dispatch (restore to pool), Engagement |

---

## 6. HTTP API

Base path: `/api/v1/compliance`

### Driver-facing (Driver App)

| Method | Path | Description |
|---|---|---|
| `GET` | `/me/profile` | Get own compliance profile + required doc list |
| `POST` | `/me/documents` | Submit a document (multipart: file + metadata) |
| `GET` | `/me/documents/{id}` | Get document detail + status (used by Driver App submission timeline screen) |

### Admin-facing (Admin Portal)

| Method | Path | Description |
|---|---|---|
| `GET` | `/admin/queue` | Paginated review queue (pending docs across all drivers) |
| `GET` | `/admin/profiles` | List all compliance profiles with filters (status, jurisdiction) |
| `GET` | `/admin/profiles/{profile_id}` | Full profile: all docs, audit log |
| `POST` | `/admin/documents/{doc_id}/approve` | Approve a document |
| `POST` | `/admin/documents/{doc_id}/reject` | Reject with reason |
| `POST` | `/admin/profiles/{profile_id}/suspend` | Manual suspension override |
| `POST` | `/admin/profiles/{profile_id}/reinstate` | Manual reinstatement |

### Internal (service-to-service)

| Method | Path | Description |
|---|---|---|
| `GET` | `/internal/status/{entity_type}/{entity_id}` | Compliance status check (dispatch cache refresh) |

---

## 7. UI Surfaces

### 7.1 Admin Portal — Compliance Console

Route: `/compliance`

- **KPI strip**: Compliant count / Pending Review / Expiring Soon / Suspended
- **Two-panel layout**: Review queue (left) + Document detail (right)
- **Review queue**: Each item shows driver name, document type, submission tag (New / Renewal / Re-submission), time ago
- **Document detail panel**: Driver header with overall status badge; list of required documents — pending review cards show full-size image button + Approve / Reject actions + rejection reason input; approved documents show approver name and timestamp
- **Expiry tab**: Calendar/list view of upcoming expirations sorted by urgency

### 7.2 Driver App — Document Submission

Entry point: Profile screen → "Documents" section

- **Compliance banner**: Status-coloured banner at top of Profile ("Action Required", "Under Review", "All Clear")
- **Document checklist**: Each required document shown as a row with status dot (red = missing, amber = expiring, green = approved, cyan = under review); tap row to open upload
- **Upload screen**: Camera capture area + manual gallery fallback; document number field; issue date + expiry date pickers; Submit button
- **Post-submission screen**: Timeline view showing Submitted → In Queue → Decision → Active; estimated review time shown

### 7.3 Partner Portal — Driver Cards (Read-Only)

- Compliance status badge on each driver card: Compliant (green) / Expiring Soon (amber, pulsing) / Suspended (red) / Under Review (cyan)
- Suspended and Under Review drivers have "Cannot Assign" disabled button
- No review or override actions — Admin Portal only

---

## 8. Non-Functional Requirements

- **File storage**: Document images stored in tenant-scoped S3 prefix; presigned URLs with 15-minute expiry for retrieval
- **File size limit**: 10 MB per document image; accepted formats: JPEG, PNG, PDF
- **Expiry checker**: Runs daily at 02:00 UTC via Tokio scheduled task; idempotent (re-running produces same state transitions)
- **Audit log retention**: Compliance audit logs retained for 7 years (regulatory requirement)
- **P99 latency**: Document submission API < 500ms (excluding file upload); review queue API < 200ms
- **Access control**: Driver endpoints require `role: driver` JWT claim; admin endpoints require `role: compliance_admin` or `role: platform_admin`; internal endpoints mTLS only

---

## 9. Follow-On Specs

| Spec | New document types | Entity type |
|---|---|---|
| Partner / Carrier Compliance | `UAE_TRADE_LICENSE`, `UAE_RTA_PERMIT`, `PH_LTFRB_FRANCHISE`, fleet insurance | `partner` |
| Merchant Compliance | `PH_TIN`, `PH_BIR_CERT`, `UAE_TRADE_LICENSE`, bank account verification | `merchant` |

Both follow-on specs reuse the same service, same API structure, and same Kafka event patterns. Only new `DocumentType` seed data and entity-specific UI surfaces are required.
