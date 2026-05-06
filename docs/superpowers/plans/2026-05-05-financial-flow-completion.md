# Financial Flow Completion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire the five missing pieces of the merchant invoice/remittance financial flow across the Rust payments service and Next.js portals.

**Architecture:** Two phases. Phase 1 adds three low-risk endpoints (billing run trigger, commission breakdown, merchant billing account CRUD) with three new migrations. Phase 2 replaces the withdrawal stub with a proper ops approval state machine and adds invoice PDF generation via headless Chrome.

**Tech Stack:** Rust / Axum / SQLx / Tokio — `services/payments`; Next.js / TypeScript — `apps/partner-portal`, `apps/admin-portal`; `chromiumoxide` + `tera` for Phase 2 PDF.

**Spec:** `docs/superpowers/specs/2026-05-05-financial-flow-completion-design.md`

---

## File Map

### Phase 1 — created
- `services/payments/migrations/0008_create_merchant_billing_accounts.sql`
- `services/payments/migrations/0009_create_partner_bonuses.sql`
- `services/payments/src/domain/entities/merchant_billing_account.rs`
- `services/payments/src/infrastructure/db/merchant_billing_account_repo.rs`
- `services/payments/src/api/http/merchant_billing_accounts.rs`
- `services/payments/src/application/queries/commission_breakdown.rs`
- `services/payments/src/infrastructure/db/partner_bonus_repo.rs`
- `services/payments/src/api/http/partner_commission.rs`

### Phase 1 — modified
- `services/payments/src/domain/entities/mod.rs` — re-export `MerchantBillingAccount`
- `services/payments/src/infrastructure/db/mod.rs` — re-export new repos
- `services/payments/src/application/queries/mod.rs` — add `commission_breakdown` module
- `services/payments/src/application/commands/mod.rs` — add `AdminRunBillingCommand`
- `services/payments/src/application/services/billing_aggregation_service.rs` — add `run_for_merchant()`
- `services/payments/src/api/http/billing.rs` — add `run_billing_admin` handler
- `services/payments/src/api/http/mod.rs` — register new routes + extend `AppState`
- `services/payments/src/bootstrap.rs` — wire new repos/services into `AppState`
- `apps/partner-portal/src/app/(dashboard)/payouts/page.tsx` — replace static chart

### Phase 2 — created
- `services/payments/migrations/0010_create_withdrawal_requests.sql`
- `services/payments/src/domain/entities/withdrawal_request.rs`
- `services/payments/src/infrastructure/db/withdrawal_request_repo.rs`
- `services/payments/src/application/services/withdrawal_service.rs`
- `services/payments/src/api/http/withdrawal_requests.rs`
- `services/payments/src/application/services/pdf_renderer.rs`
- `services/payments/src/api/http/invoice_pdf.rs`
- `services/payments/templates/invoice.html`

### Phase 2 — modified
- `services/payments/src/domain/entities/wallet.rs` — add `reserved_centavos`
- `services/payments/src/infrastructure/db/wallet_repo.rs` — include `reserved_centavos`
- `services/payments/src/application/commands/mod.rs` — update `WalletSummary`, add withdrawal commands
- `services/payments/src/application/services/wallet_service.rs` — update `summary()`, rework `request_withdrawal()`
- `services/payments/src/api/http/wallet.rs` — rework `request_withdrawal` handler return type
- `services/payments/src/api/http/mod.rs` — add withdrawal + PDF routes, extend `AppState`
- `services/payments/src/application/services/mod.rs` — export `WithdrawalService`, `PdfRenderer`
- `services/payments/src/bootstrap.rs` — wire `WithdrawalService`, `PdfRenderer`
- `services/payments/Cargo.toml` — add `chromiumoxide`, `tera`

---

## PHASE 1

---

### Task 1: Phase 1 Migrations

**Files:**
- Create: `services/payments/migrations/0008_create_merchant_billing_accounts.sql`
- Create: `services/payments/migrations/0009_create_partner_bonuses.sql`

- [ ] **Step 1: Write migration 0008**

```sql
-- services/payments/migrations/0008_create_merchant_billing_accounts.sql
CREATE TABLE payments.merchant_billing_accounts (
  id                          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id                   UUID NOT NULL,
  merchant_id                 UUID NOT NULL UNIQUE,
  base_rate_override_centavos INT,
  payment_terms_days          SMALLINT NOT NULL DEFAULT 30,
  credit_limit_centavos       BIGINT NOT NULL DEFAULT 0,
  tin                         VARCHAR(20),
  vat_registered              BOOLEAN NOT NULL DEFAULT false,
  billing_email               TEXT NOT NULL,
  invoice_channel             TEXT NOT NULL DEFAULT 'email',
  bank_name                   TEXT,
  bank_account_number         TEXT,
  bank_account_name           TEXT,
  created_at                  TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at                  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX ON payments.merchant_billing_accounts (tenant_id);
```

- [ ] **Step 2: Write migration 0009**

```sql
-- services/payments/migrations/0009_create_partner_bonuses.sql
CREATE TABLE payments.partner_bonuses (
  id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id        UUID NOT NULL,
  partner_id       UUID NOT NULL,
  amount_centavos  BIGINT NOT NULL,
  currency         CHAR(3) NOT NULL DEFAULT 'PHP',
  reason           TEXT NOT NULL,
  effective_month  DATE NOT NULL,
  created_by       UUID NOT NULL,
  created_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX ON payments.partner_bonuses (partner_id, effective_month);
```

- [ ] **Step 3: Verify migrations apply cleanly**

Run: `cd D:\LogisticOS\services\payments && cargo sqlx migrate run`
Expected: exits 0, both tables created.

- [ ] **Step 4: Commit**

```bash
git add services/payments/migrations/0008_create_merchant_billing_accounts.sql
git add services/payments/migrations/0009_create_partner_bonuses.sql
git commit -m "feat(payments): add merchant_billing_accounts and partner_bonuses migrations"
```

---

### Task 2: MerchantBillingAccount Entity + Repository Trait

**Files:**
- Create: `services/payments/src/domain/entities/merchant_billing_account.rs`
- Modify: `services/payments/src/domain/entities/mod.rs`
- Modify: `services/payments/src/domain/repositories/mod.rs`

- [ ] **Step 1: Write the domain entity**

Create `services/payments/src/domain/entities/merchant_billing_account.rs`:

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerchantBillingAccount {
    pub id:                          Uuid,
    pub tenant_id:                   Uuid,
    pub merchant_id:                 Uuid,
    pub base_rate_override_centavos: Option<i32>,
    pub payment_terms_days:          i16,
    pub credit_limit_centavos:       i64,
    pub tin:                         Option<String>,
    pub vat_registered:              bool,
    pub billing_email:               String,
    pub invoice_channel:             String,
    pub bank_name:                   Option<String>,
    /// Stored in full; masked to last-4 on GET responses.
    pub bank_account_number:         Option<String>,
    pub bank_account_name:           Option<String>,
    pub created_at:                  DateTime<Utc>,
    pub updated_at:                  DateTime<Utc>,
}

impl MerchantBillingAccount {
    pub fn new(
        tenant_id:     Uuid,
        merchant_id:   Uuid,
        billing_email: String,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            merchant_id,
            base_rate_override_centavos: None,
            payment_terms_days: 30,
            credit_limit_centavos: 0,
            tin: None,
            vat_registered: false,
            billing_email,
            invoice_channel: "email".into(),
            bank_name: None,
            bank_account_number: None,
            bank_account_name: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Returns bank account number masked to last 4 digits.
    pub fn masked_bank_account(&self) -> Option<String> {
        self.bank_account_number.as_ref().map(|n| {
            let digits: String = n.chars().filter(|c| c.is_ascii_digit()).collect();
            if digits.len() <= 4 {
                "*".repeat(digits.len())
            } else {
                format!("****{}", &digits[digits.len() - 4..])
            }
        })
    }
}
```

- [ ] **Step 2: Write failing unit test**

Add to the bottom of `merchant_billing_account.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masked_bank_account_shows_last_4() {
        let mut acct = MerchantBillingAccount::new(
            Uuid::new_v4(), Uuid::new_v4(), "m@example.com".into()
        );
        acct.bank_account_number = Some("1234567890".into());
        assert_eq!(acct.masked_bank_account(), Some("****7890".into()));
    }

    #[test]
    fn masked_bank_account_none_when_absent() {
        let acct = MerchantBillingAccount::new(
            Uuid::new_v4(), Uuid::new_v4(), "m@example.com".into()
        );
        assert_eq!(acct.masked_bank_account(), None);
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cd D:\LogisticOS\services\payments && cargo test domain::entities::merchant_billing_account`
Expected: 2 tests pass.

- [ ] **Step 4: Register in entities mod**

In `services/payments/src/domain/entities/mod.rs`, append:

```rust
pub mod merchant_billing_account;
pub use merchant_billing_account::MerchantBillingAccount;
```

- [ ] **Step 5: Add repository trait**

In `services/payments/src/domain/repositories/mod.rs`, append:

```rust
#[async_trait]
pub trait MerchantBillingAccountRepository: Send + Sync {
    async fn find_by_merchant(&self, merchant_id: Uuid) -> anyhow::Result<Option<crate::domain::entities::MerchantBillingAccount>>;
    async fn upsert(&self, account: &crate::domain::entities::MerchantBillingAccount) -> anyhow::Result<()>;
}
```

- [ ] **Step 6: Commit**

```bash
git add services/payments/src/domain/entities/merchant_billing_account.rs
git add services/payments/src/domain/entities/mod.rs
git add services/payments/src/domain/repositories/mod.rs
git commit -m "feat(payments): MerchantBillingAccount domain entity and repository trait"
```

---

### Task 3: PgMerchantBillingAccountRepository

**Files:**
- Create: `services/payments/src/infrastructure/db/merchant_billing_account_repo.rs`
- Modify: `services/payments/src/infrastructure/db/mod.rs`

- [ ] **Step 1: Write the repo**

Create `services/payments/src/infrastructure/db/merchant_billing_account_repo.rs`:

```rust
use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;
use crate::domain::{
    entities::MerchantBillingAccount,
    repositories::MerchantBillingAccountRepository,
};

pub struct PgMerchantBillingAccountRepository { pool: PgPool }
impl PgMerchantBillingAccountRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[derive(sqlx::FromRow)]
struct AccountRow {
    id:                          Uuid,
    tenant_id:                   Uuid,
    merchant_id:                 Uuid,
    base_rate_override_centavos: Option<i32>,
    payment_terms_days:          i16,
    credit_limit_centavos:       i64,
    tin:                         Option<String>,
    vat_registered:              bool,
    billing_email:               String,
    invoice_channel:             String,
    bank_name:                   Option<String>,
    bank_account_number:         Option<String>,
    bank_account_name:           Option<String>,
    created_at:                  chrono::DateTime<chrono::Utc>,
    updated_at:                  chrono::DateTime<chrono::Utc>,
}

impl From<AccountRow> for MerchantBillingAccount {
    fn from(r: AccountRow) -> Self {
        MerchantBillingAccount {
            id: r.id,
            tenant_id: r.tenant_id,
            merchant_id: r.merchant_id,
            base_rate_override_centavos: r.base_rate_override_centavos,
            payment_terms_days: r.payment_terms_days,
            credit_limit_centavos: r.credit_limit_centavos,
            tin: r.tin,
            vat_registered: r.vat_registered,
            billing_email: r.billing_email,
            invoice_channel: r.invoice_channel,
            bank_name: r.bank_name,
            bank_account_number: r.bank_account_number,
            bank_account_name: r.bank_account_name,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

const SELECT: &str = "SELECT id, tenant_id, merchant_id, base_rate_override_centavos,
    payment_terms_days, credit_limit_centavos, tin, vat_registered, billing_email,
    invoice_channel, bank_name, bank_account_number, bank_account_name,
    created_at, updated_at FROM payments.merchant_billing_accounts";

#[async_trait]
impl MerchantBillingAccountRepository for PgMerchantBillingAccountRepository {
    async fn find_by_merchant(&self, merchant_id: Uuid) -> anyhow::Result<Option<MerchantBillingAccount>> {
        let row = sqlx::query_as::<_, AccountRow>(
            &format!("{SELECT} WHERE merchant_id = $1")
        ).bind(merchant_id).fetch_optional(&self.pool).await?;
        Ok(row.map(MerchantBillingAccount::from))
    }

    async fn upsert(&self, a: &MerchantBillingAccount) -> anyhow::Result<()> {
        sqlx::query(
            r#"INSERT INTO payments.merchant_billing_accounts
                (id, tenant_id, merchant_id, base_rate_override_centavos,
                 payment_terms_days, credit_limit_centavos, tin, vat_registered,
                 billing_email, invoice_channel, bank_name, bank_account_number,
                 bank_account_name, created_at, updated_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)
               ON CONFLICT (merchant_id) DO UPDATE SET
                 base_rate_override_centavos = EXCLUDED.base_rate_override_centavos,
                 payment_terms_days          = EXCLUDED.payment_terms_days,
                 credit_limit_centavos       = EXCLUDED.credit_limit_centavos,
                 tin                         = EXCLUDED.tin,
                 vat_registered              = EXCLUDED.vat_registered,
                 billing_email               = EXCLUDED.billing_email,
                 invoice_channel             = EXCLUDED.invoice_channel,
                 bank_name                   = EXCLUDED.bank_name,
                 bank_account_number         = EXCLUDED.bank_account_number,
                 bank_account_name           = EXCLUDED.bank_account_name,
                 updated_at                  = EXCLUDED.updated_at"#
        )
        .bind(a.id).bind(a.tenant_id).bind(a.merchant_id)
        .bind(a.base_rate_override_centavos).bind(a.payment_terms_days)
        .bind(a.credit_limit_centavos).bind(a.tin.as_deref())
        .bind(a.vat_registered).bind(&a.billing_email).bind(&a.invoice_channel)
        .bind(a.bank_name.as_deref()).bind(a.bank_account_number.as_deref())
        .bind(a.bank_account_name.as_deref()).bind(a.created_at).bind(a.updated_at)
        .execute(&self.pool).await?;
        Ok(())
    }
}
```

- [ ] **Step 2: Register in db mod**

In `services/payments/src/infrastructure/db/mod.rs`, append:

```rust
pub mod merchant_billing_account_repo;
pub use merchant_billing_account_repo::PgMerchantBillingAccountRepository;
```

- [ ] **Step 3: Verify compile**

Run: `cd D:\LogisticOS\services\payments && cargo check`
Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add services/payments/src/infrastructure/db/merchant_billing_account_repo.rs
git add services/payments/src/infrastructure/db/mod.rs
git commit -m "feat(payments): PgMerchantBillingAccountRepository"
```

---

### Task 4: Merchant Billing Account HTTP Handlers

**Files:**
- Create: `services/payments/src/api/http/merchant_billing_accounts.rs`
- Modify: `services/payments/src/api/http/mod.rs`

- [ ] **Step 1: Write handlers**

Create `services/payments/src/api/http/merchant_billing_accounts.rs`:

```rust
use axum::{extract::{Path, State}, Json, http::StatusCode};
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use logisticos_auth::middleware::AuthClaims;
use logisticos_auth::require_permission;
use logisticos_errors::AppError;
use chrono::Utc;
use crate::{
    api::http::AppState,
    domain::entities::MerchantBillingAccount,
};

/// GET /v1/admin/merchants/:merchant_id/billing-account
pub async fn get_billing_account(
    AuthClaims(claims): AuthClaims,
    Path(merchant_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::BILLING_ADMIN);
    let acct = state.merchant_billing_account_repo
        .find_by_merchant(merchant_id).await
        .map_err(AppError::Internal)?
        .ok_or_else(|| AppError::NotFound(format!("No billing account for merchant {merchant_id}")))?;

    Ok(Json(account_to_json(&acct, true)))
}

/// POST /v1/admin/merchants/:merchant_id/billing-account  (upsert)
pub async fn upsert_billing_account(
    AuthClaims(claims): AuthClaims,
    Path(merchant_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<UpsertBillingAccountBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::BILLING_ADMIN);

    let existing = state.merchant_billing_account_repo
        .find_by_merchant(merchant_id).await
        .map_err(AppError::Internal)?;

    let mut acct = existing.unwrap_or_else(|| {
        MerchantBillingAccount::new(
            claims.tenant_id,
            merchant_id,
            body.billing_email.clone().unwrap_or_default(),
        )
    });

    apply_body(&mut acct, body);
    acct.updated_at = Utc::now();

    state.merchant_billing_account_repo.upsert(&acct).await.map_err(AppError::Internal)?;

    let is_new = acct.created_at == acct.updated_at;
    let status = if is_new { StatusCode::CREATED } else { StatusCode::OK };
    Ok((status, Json(account_to_json(&acct, true))))
}

/// PATCH /v1/admin/merchants/:merchant_id/billing-account  (partial update)
pub async fn patch_billing_account(
    AuthClaims(claims): AuthClaims,
    Path(merchant_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<UpsertBillingAccountBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::BILLING_ADMIN);

    let mut acct = state.merchant_billing_account_repo
        .find_by_merchant(merchant_id).await
        .map_err(AppError::Internal)?
        .ok_or_else(|| AppError::NotFound(format!("No billing account for merchant {merchant_id}")))?;

    apply_body(&mut acct, body);
    acct.updated_at = Utc::now();
    state.merchant_billing_account_repo.upsert(&acct).await.map_err(AppError::Internal)?;

    Ok(Json(account_to_json(&acct, true)))
}

// ── Shared helpers ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct UpsertBillingAccountBody {
    pub base_rate_override_centavos: Option<i32>,
    pub payment_terms_days:          Option<i16>,
    pub credit_limit_centavos:       Option<i64>,
    pub tin:                         Option<String>,
    pub vat_registered:              Option<bool>,
    pub billing_email:               Option<String>,
    pub invoice_channel:             Option<String>,
    pub bank_name:                   Option<String>,
    pub bank_account_number:         Option<String>,
    pub bank_account_name:           Option<String>,
}

fn apply_body(acct: &mut MerchantBillingAccount, body: UpsertBillingAccountBody) {
    if let Some(v) = body.base_rate_override_centavos { acct.base_rate_override_centavos = Some(v); }
    if let Some(v) = body.payment_terms_days          { acct.payment_terms_days = v; }
    if let Some(v) = body.credit_limit_centavos       { acct.credit_limit_centavos = v; }
    if let Some(v) = body.tin                         { acct.tin = Some(v); }
    if let Some(v) = body.vat_registered              { acct.vat_registered = v; }
    if let Some(v) = body.billing_email               { acct.billing_email = v; }
    if let Some(v) = body.invoice_channel             { acct.invoice_channel = v; }
    if let Some(v) = body.bank_name                   { acct.bank_name = Some(v); }
    if let Some(v) = body.bank_account_number         { acct.bank_account_number = Some(v); }
    if let Some(v) = body.bank_account_name           { acct.bank_account_name = Some(v); }
}

fn account_to_json(a: &MerchantBillingAccount, mask_bank: bool) -> serde_json::Value {
    serde_json::json!({
        "id":                          a.id,
        "merchant_id":                 a.merchant_id,
        "base_rate_override_centavos": a.base_rate_override_centavos,
        "payment_terms_days":          a.payment_terms_days,
        "credit_limit_centavos":       a.credit_limit_centavos,
        "tin":                         a.tin,
        "vat_registered":              a.vat_registered,
        "billing_email":               a.billing_email,
        "invoice_channel":             a.invoice_channel,
        "bank_name":                   a.bank_name,
        "bank_account_number":         if mask_bank { a.masked_bank_account() } else { a.bank_account_number.clone() },
        "bank_account_name":           a.bank_account_name,
        "updated_at":                  a.updated_at.to_rfc3339(),
    })
}
```

- [ ] **Step 2: Register in AppState and router**

In `services/payments/src/api/http/mod.rs`:

Add `pub mod merchant_billing_accounts;` at the top.

Add `merchant_billing_account_repo` field to `AppState`:
```rust
pub merchant_billing_account_repo: Arc<dyn crate::domain::repositories::MerchantBillingAccountRepository>,
```

In `protected_router`, add under the existing routes:
```rust
.route("/admin/merchants/:merchant_id/billing-account",
    get(merchant_billing_accounts::get_billing_account)
    .post(merchant_billing_accounts::upsert_billing_account)
    .patch(merchant_billing_accounts::patch_billing_account))
```

- [ ] **Step 3: Wire in bootstrap**

In `services/payments/src/bootstrap.rs`, after the existing repo declarations:
```rust
let merchant_billing_account_repo = Arc::new(
    PgMerchantBillingAccountRepository::new(pool.clone())
);
```

Add `merchant_billing_account_repo: Arc::clone(&merchant_billing_account_repo) as _,` to the `AppState` construction.

Update the import line to include `PgMerchantBillingAccountRepository`.

- [ ] **Step 4: Verify compile**

Run: `cd D:\LogisticOS\services\payments && cargo check`
Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add services/payments/src/api/http/merchant_billing_accounts.rs
git add services/payments/src/api/http/mod.rs
git add services/payments/src/bootstrap.rs
git commit -m "feat(payments): merchant billing account CRUD endpoints"
```

---

### Task 5: Partner Bonus Repo + Commission Breakdown

**Files:**
- Create: `services/payments/src/infrastructure/db/partner_bonus_repo.rs`
- Create: `services/payments/src/application/queries/commission_breakdown.rs`
- Create: `services/payments/src/api/http/partner_commission.rs`
- Modify: `services/payments/src/infrastructure/db/mod.rs`
- Modify: `services/payments/src/application/queries/mod.rs`
- Modify: `services/payments/src/api/http/mod.rs`

- [ ] **Step 1: Write the partner bonus repo**

Create `services/payments/src/infrastructure/db/partner_bonus_repo.rs`:

```rust
use sqlx::PgPool;
use uuid::Uuid;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PartnerBonus {
    pub id:              Uuid,
    pub tenant_id:       Uuid,
    pub partner_id:      Uuid,
    pub amount_centavos: i64,
    pub currency:        String,
    pub reason:          String,
    pub effective_month: NaiveDate,
    pub created_by:      Uuid,
    pub created_at:      chrono::DateTime<chrono::Utc>,
}

pub struct PgPartnerBonusRepo { pool: PgPool }
impl PgPartnerBonusRepo { pub fn new(pool: PgPool) -> Self { Self { pool } } }

impl PgPartnerBonusRepo {
    pub async fn insert(&self, b: &PartnerBonus) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO payments.partner_bonuses
             (id, tenant_id, partner_id, amount_centavos, currency, reason,
              effective_month, created_by, created_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)"
        )
        .bind(b.id).bind(b.tenant_id).bind(b.partner_id).bind(b.amount_centavos)
        .bind(&b.currency).bind(&b.reason).bind(b.effective_month)
        .bind(b.created_by).bind(b.created_at)
        .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn sum_for_partner_month(
        &self,
        partner_id: Uuid,
        month_start: NaiveDate,
    ) -> anyhow::Result<i64> {
        let row: (Option<i64>,) = sqlx::query_as(
            "SELECT COALESCE(SUM(amount_centavos), 0)
             FROM payments.partner_bonuses
             WHERE partner_id = $1
               AND date_trunc('month', effective_month) = date_trunc('month', $2::date)"
        ).bind(partner_id).bind(month_start).fetch_one(&self.pool).await?;
        Ok(row.0.unwrap_or(0))
    }
}
```

- [ ] **Step 2: Write the commission breakdown query**

Create `services/payments/src/application/queries/commission_breakdown.rs`:

```rust
//! Commission breakdown query — aggregates base charges, COD remittances,
//! and bonuses for a partner in a given month.
//!
//! Invoice line items are stored as JSONB, so totals are loaded per invoice
//! and summed in Rust. COD and bonuses are simple SQL aggregations.

use chrono::NaiveDate;
use sqlx::PgPool;
use uuid::Uuid;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct CommissionBreakdown {
    pub period:                    String,   // "YYYY-MM"
    pub base_charges_centavos:     i64,
    pub cod_remittance_centavos:   i64,
    pub bonuses_centavos:          i64,
    pub total_centavos:            i64,
    pub currency:                  String,
}

pub struct CommissionBreakdownQuery { pool: PgPool }
impl CommissionBreakdownQuery {
    pub fn new(pool: PgPool) -> Self { Self { pool } }

    pub async fn run(
        &self,
        partner_id:  Uuid,
        year:        i32,
        month:       u32,
    ) -> anyhow::Result<CommissionBreakdown> {
        let month_start = NaiveDate::from_ymd_opt(year, month, 1)
            .ok_or_else(|| anyhow::anyhow!("invalid year/month: {year}-{month}"))?;

        // ── Base charges: sum total_due across ShipmentCharges invoices for the
        //    partner's merchants in this month. Invoices store line items as JSONB
        //    so totals are computed via Invoice::total_due() in Rust.
        //    Cross-schema join: identity.merchant_memberships links partner→merchant.
        let invoice_rows: Vec<(serde_json::Value, serde_json::Value)> = sqlx::query_as(
            r#"SELECT i.line_items, i.adjustments
               FROM payments.invoices i
               JOIN identity.merchant_memberships mm ON mm.merchant_id = i.merchant_id
               WHERE mm.partner_id = $1
                 AND invoice_type = 'shipment_charges'
                 AND status IN ('issued', 'paid', 'overdue')
                 AND date_trunc('month', billing_start) = date_trunc('month', $2::date)"#
        ).bind(partner_id).bind(month_start).fetch_all(&self.pool).await?;

        let base_charges_centavos = invoice_rows.iter().map(|(items_json, adjs_json)| {
            let items: Vec<crate::domain::entities::InvoiceLineItem> =
                serde_json::from_value(items_json.clone()).unwrap_or_default();
            let adjs: Vec<crate::domain::entities::InvoiceAdjustment> =
                serde_json::from_value(adjs_json.clone()).unwrap_or_default();
            let subtotal: i64 = items.iter().map(|i| i.net().amount).sum();
            let adj_total: i64 = adjs.iter().map(|a| a.amount.amount).sum();
            let taxable = subtotal + adj_total;
            let vat = (taxable as f64 * 0.12).round() as i64;
            taxable + vat
        }).sum::<i64>();

        // ── COD remittance: sum net_cents of Paid batches for partner's merchants.
        let (cod_centavos,): (Option<i64>,) = sqlx::query_as(
            r#"SELECT COALESCE(SUM(b.net_cents), 0)
               FROM payments.cod_remittance_batches b
               JOIN identity.merchant_memberships mm ON mm.merchant_id = b.merchant_id
               WHERE mm.partner_id = $1
                 AND b.status = 'paid'
                 AND date_trunc('month', b.paid_at) = date_trunc('month', $2::date)"#
        ).bind(partner_id).bind(month_start).fetch_one(&self.pool).await?;
        let cod_centavos = cod_centavos.unwrap_or(0);

        // ── Bonuses: direct aggregate on partner_bonuses table.
        let (bonuses_centavos,): (Option<i64>,) = sqlx::query_as(
            "SELECT COALESCE(SUM(amount_centavos), 0)
             FROM payments.partner_bonuses
             WHERE partner_id = $1
               AND date_trunc('month', effective_month) = date_trunc('month', $2::date)"
        ).bind(partner_id).bind(month_start).fetch_one(&self.pool).await?;
        let bonuses_centavos = bonuses_centavos.unwrap_or(0);

        let total = base_charges_centavos + cod_centavos + bonuses_centavos;

        Ok(CommissionBreakdown {
            period:                  format!("{year}-{month:02}"),
            base_charges_centavos,
            cod_remittance_centavos: cod_centavos,
            bonuses_centavos,
            total_centavos:          total,
            currency:                "PHP".into(),
        })
    }
}
```

- [ ] **Step 3: Write HTTP handlers**

Create `services/payments/src/api/http/partner_commission.rs`:

```rust
use axum::{extract::{Query, State}, Json, http::StatusCode};
use std::sync::Arc;
use serde::Deserialize;
use uuid::Uuid;
use logisticos_auth::middleware::AuthClaims;
use logisticos_auth::require_permission;
use logisticos_errors::AppError;
use crate::api::http::AppState;
use crate::infrastructure::db::partner_bonus_repo::PartnerBonus;

#[derive(Deserialize)]
pub struct BreakdownParams {
    partner_id: Uuid,
    year:       i32,
    month:      u32,
}

/// GET /v1/partner/commission/breakdown
pub async fn get_commission_breakdown(
    AuthClaims(claims): AuthClaims,
    Query(params): Query<BreakdownParams>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::BILLING_VIEW);
    let breakdown = state.commission_query
        .run(params.partner_id, params.year, params.month).await
        .map_err(AppError::Internal)?;
    Ok(Json(serde_json::json!({ "data": breakdown })))
}

#[derive(Deserialize)]
pub struct CreateBonusBody {
    pub partner_id:      Uuid,
    pub amount_centavos: i64,
    pub reason:          String,
    pub effective_month: chrono::NaiveDate,
}

/// POST /v1/admin/partner-bonuses
pub async fn create_partner_bonus(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateBonusBody>,
) -> Result<StatusCode, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::BILLING_ADMIN);
    if body.amount_centavos <= 0 {
        return Err(AppError::Validation("amount_centavos must be positive".into()));
    }
    let bonus = PartnerBonus {
        id:              uuid::Uuid::new_v4(),
        tenant_id:       claims.tenant_id,
        partner_id:      body.partner_id,
        amount_centavos: body.amount_centavos,
        currency:        "PHP".into(),
        reason:          body.reason,
        effective_month: body.effective_month,
        created_by:      claims.user_id,
        created_at:      chrono::Utc::now(),
    };
    state.partner_bonus_repo.insert(&bonus).await.map_err(AppError::Internal)?;
    Ok(StatusCode::CREATED)
}
```

- [ ] **Step 4: Register modules**

In `services/payments/src/infrastructure/db/mod.rs`, append:
```rust
pub mod partner_bonus_repo;
pub use partner_bonus_repo::PgPartnerBonusRepo;
```

In `services/payments/src/application/queries/mod.rs`, replace content with:
```rust
// Read-side: COD batch reports, revenue analytics, payout history.
pub mod commission_breakdown;
pub use commission_breakdown::CommissionBreakdownQuery;
```

In `services/payments/src/api/http/mod.rs`, add `pub mod partner_commission;` at top.

Add to `AppState`:
```rust
pub commission_query:       Arc<crate::application::queries::CommissionBreakdownQuery>,
pub partner_bonus_repo:     Arc<crate::infrastructure::db::partner_bonus_repo::PgPartnerBonusRepo>,
```

In `protected_router`, append routes:
```rust
.route("/partner/commission/breakdown", get(partner_commission::get_commission_breakdown))
.route("/admin/partner-bonuses",        post(partner_commission::create_partner_bonus))
```

- [ ] **Step 5: Wire in bootstrap**

In `services/payments/src/bootstrap.rs`, after existing repo declarations:
```rust
let partner_bonus_repo   = Arc::new(crate::infrastructure::db::partner_bonus_repo::PgPartnerBonusRepo::new(pool.clone()));
let commission_query     = Arc::new(crate::application::queries::CommissionBreakdownQuery::new(pool.clone()));
```

Add to `AppState`:
```rust
commission_query:       Arc::clone(&commission_query),
partner_bonus_repo:     Arc::clone(&partner_bonus_repo),
```

- [ ] **Step 6: Verify compile**

Run: `cd D:\LogisticOS\services\payments && cargo check`
Expected: no errors.

- [ ] **Step 7: Commit**

```bash
git add services/payments/src/infrastructure/db/partner_bonus_repo.rs
git add services/payments/src/infrastructure/db/mod.rs
git add services/payments/src/application/queries/commission_breakdown.rs
git add services/payments/src/application/queries/mod.rs
git add services/payments/src/api/http/partner_commission.rs
git add services/payments/src/api/http/mod.rs
git add services/payments/src/bootstrap.rs
git commit -m "feat(payments): partner bonus repo and commission breakdown endpoint"
```

---

### Task 6: Admin Billing Run Trigger

**Files:**
- Modify: `services/payments/src/application/commands/mod.rs`
- Modify: `services/payments/src/application/services/billing_aggregation_service.rs`
- Modify: `services/payments/src/api/http/billing.rs`
- Modify: `services/payments/src/api/http/mod.rs`

- [ ] **Step 1: Add AdminRunBillingCommand**

In `services/payments/src/application/commands/mod.rs`, append:

```rust
/// Admin-initiated billing run for a single merchant over an explicit date range.
/// Exposed via `POST /v1/admin/billing/run` (requires BILLING_ADMIN).
#[derive(Debug, Deserialize)]
pub struct AdminRunBillingCommand {
    pub merchant_id:    Uuid,
    pub merchant_email: Option<String>,
    pub tenant_code:    String,
    pub period_start:   NaiveDate,
    pub period_end:     NaiveDate,
}
```

- [ ] **Step 2: Add `run_for_merchant` to BillingAggregationService**

In `services/payments/src/application/services/billing_aggregation_service.rs`, add the import at the top:
```rust
use crate::application::commands::AdminRunBillingCommand;
```

Append after the `run_monthly` method (before the closing `}` of `impl BillingAggregationService`):

```rust
/// Admin-triggered billing run with an explicit date range instead of year/month.
/// Delegates to the same invoice + billing-run logic as `run_monthly`.
pub async fn run_for_merchant(
    &self,
    tenant_id: &TenantId,
    cmd: AdminRunBillingCommand,
) -> AppResult<(BillingRunRecord, BillingRunOutcome)> {
    if cmd.period_end < cmd.period_start {
        return Err(AppError::Validation("period_end must be >= period_start".into()));
    }

    let merchant_id = MerchantId::from_uuid(cmd.merchant_id);

    // ── Idempotency check ─────────────────────────────────────────────────
    if let Some(existing) = self.runs
        .find_for_period(tenant_id, &merchant_id, cmd.period_start, cmd.period_end)
        .await.map_err(AppError::Internal)?
    {
        return Ok((existing, BillingRunOutcome::AlreadyExisted));
    }

    // ── Pull delivered shipments ──────────────────────────────────────────
    let from_utc = Utc.from_utc_datetime(&cmd.period_start.and_time(chrono::NaiveTime::MIN));
    let to_utc   = Utc.from_utc_datetime(
        &(cmd.period_end + Duration::days(1)).and_time(chrono::NaiveTime::MIN),
    );
    let shipments = self.billing_source
        .list_delivered(tenant_id.inner(), cmd.merchant_id, from_utc, to_utc)
        .await.map_err(AppError::Internal)?;

    if shipments.is_empty() {
        let run = BillingRunRecord {
            id:             uuid::Uuid::new_v4(),
            tenant_id:      tenant_id.clone(),
            merchant_id:    merchant_id.clone(),
            period_start:   cmd.period_start,
            period_end:     cmd.period_end,
            invoice_id:     None,
            shipment_count: 0,
            total_cents:    0,
            created_at:     Utc::now(),
        };
        self.runs.save(&run).await.map_err(AppError::Internal)?;
        return Ok((run, BillingRunOutcome::NoShipments));
    }

    // ── Build charges ─────────────────────────────────────────────────────
    let mut charges = Vec::with_capacity(shipments.len() * 3);
    let mut total_cents = 0i64;
    for s in &shipments {
        total_cents = total_cents.saturating_add(s.total_cents);
        if s.base_freight_cents > 0 {
            charges.push(AwbChargeInput { awb: s.awb.clone(), charge_type: "base_freight".into(), description: "Base freight".into(), quantity: 1, unit_price_cents: s.base_freight_cents, discount_cents: None });
        }
        if s.fuel_surcharge_cents > 0 {
            charges.push(AwbChargeInput { awb: s.awb.clone(), charge_type: "fuel_surcharge".into(), description: "Fuel surcharge".into(), quantity: 1, unit_price_cents: s.fuel_surcharge_cents, discount_cents: None });
        }
        if s.insurance_cents > 0 {
            charges.push(AwbChargeInput { awb: s.awb.clone(), charge_type: "insurance_fee".into(), description: "Shipment insurance".into(), quantity: 1, unit_price_cents: s.insurance_cents, discount_cents: None });
        }
    }

    let invoice = self.invoice_service.generate(
        tenant_id,
        GenerateInvoiceCommand {
            merchant_id:          cmd.merchant_id,
            merchant_email:       cmd.merchant_email,
            tenant_code:          cmd.tenant_code,
            billing_period_year:  cmd.period_start.year(),
            billing_period_month: cmd.period_start.month(),
            charges,
        },
    ).await?;

    let run = BillingRunRecord {
        id:             uuid::Uuid::new_v4(),
        tenant_id:      tenant_id.clone(),
        merchant_id,
        period_start:   cmd.period_start,
        period_end:     cmd.period_end,
        invoice_id:     Some(invoice.id.clone()),
        shipment_count: shipments.len() as i32,
        total_cents,
        created_at:     Utc::now(),
    };
    self.runs.save(&run).await.map_err(AppError::Internal)?;
    Ok((run, BillingRunOutcome::Issued))
}
```

- [ ] **Step 3: Add admin handler**

In `services/payments/src/api/http/billing.rs`, append:

```rust
use logisticos_auth::middleware::AuthClaims;
use logisticos_auth::require_permission;
use logisticos_types::TenantId;
use crate::application::commands::AdminRunBillingCommand;

/// `POST /v1/admin/billing/run`
/// Admin-triggered billing run for a single merchant over an explicit date range.
pub async fn run_billing_admin(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
    Json(cmd): Json<AdminRunBillingCommand>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::BILLING_ADMIN);
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let (run, outcome) = state.billing_service.run_for_merchant(&tenant_id, cmd).await?;

    let outcome_str = match outcome {
        BillingRunOutcome::Issued         => "issued",
        BillingRunOutcome::AlreadyExisted => "already_existed",
        BillingRunOutcome::NoShipments    => "no_shipments",
    };
    let status = match outcome {
        BillingRunOutcome::Issued         => StatusCode::CREATED,
        BillingRunOutcome::AlreadyExisted => StatusCode::OK,
        BillingRunOutcome::NoShipments    => StatusCode::OK,
    };
    Ok((status, Json(serde_json::json!({
        "outcome":        outcome_str,
        "run_id":         run.id,
        "invoice_id":     run.invoice_id.map(|i| i.inner()),
        "period_start":   run.period_start.to_string(),
        "period_end":     run.period_end.to_string(),
        "shipment_count": run.shipment_count,
        "total_cents":    run.total_cents,
    }))))
}
```

- [ ] **Step 4: Register route**

In `services/payments/src/api/http/mod.rs`, in `protected_router`, append:
```rust
.route("/admin/billing/run", post(billing::run_billing_admin))
```

- [ ] **Step 5: Verify compile + tests**

Run: `cd D:\LogisticOS\services\payments && cargo test`
Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add services/payments/src/application/commands/mod.rs
git add services/payments/src/application/services/billing_aggregation_service.rs
git add services/payments/src/api/http/billing.rs
git add services/payments/src/api/http/mod.rs
git commit -m "feat(payments): admin billing run trigger — POST /v1/admin/billing/run"
```

---

### Task 7: Partner Portal — Replace Static Chart with Live Data

**Files:**
- Modify: `apps/partner-portal/src/app/(dashboard)/payouts/page.tsx`

- [ ] **Step 1: Add commission API call**

In [payouts/page.tsx](apps/partner-portal/src/app/(dashboard)/payouts/page.tsx), add a new fetch in the `load` callback.

Replace the existing `load` function (lines 134–152) with:

```typescript
const load = useCallback(async () => {
  setLoading(true);
  setError(null);
  try {
    const [w, txs, invs] = await Promise.all([
      paymentsApi.getWallet(),
      paymentsApi.getTransactions(),
      paymentsApi.getInvoices(),
    ]);
    setWallet(w);
    setTransactions(txs);
    setInvoices(invs);

    // Fetch last 6 months of commission breakdown
    const now = new Date();
    const months = Array.from({ length: 6 }, (_, i) => {
      const d = new Date(Date.UTC(now.getUTCFullYear(), now.getUTCMonth() - (5 - i), 1));
      return { year: d.getUTCFullYear(), month: d.getUTCMonth() + 1, label: d.toLocaleString("en", { month: "short", timeZone: "UTC" }) };
    });
    const breakdowns = await Promise.all(
      months.map(({ year, month }) =>
        paymentsApi.getCommissionBreakdown({ partner_id: w.partner_id, year, month })
          .catch(() => null)
      )
    );
    const liveChart = months.map(({ label }, i) => {
      const b = breakdowns[i];
      return {
        month: label,
        base:  b ? Math.round(b.base_charges_centavos / 100) : 0,
        cod:   b ? Math.round(b.cod_remittance_centavos / 100) : 0,
        bonus: b ? Math.round(b.bonuses_centavos / 100) : 0,
      };
    });
    setMonthlyChart(liveChart);
  } catch (e) {
    const err = e as { message?: string };
    setError(err?.message ?? "Failed to load payout data");
  } finally {
    setLoading(false);
  }
}, []);
```

Add `monthlyChart` state and remove the static `MONTHLY_PAYOUTS` constant. Add at the top of `PayoutsPage` (after existing `useState` calls):
```typescript
const [monthlyChart, setMonthlyChart] = useState(MONTHLY_PAYOUTS);
```

Replace the inline `monthlyChart` derivation (lines 175–184) — the state variable now replaces it.

Replace the static constant `MONTHLY_PAYOUTS` at line 27 with a seed used only as initial value:
```typescript
const MONTHLY_PAYOUTS = [
  { month: "–", base: 0, cod: 0, bonus: 0 },
  { month: "–", base: 0, cod: 0, bonus: 0 },
  { month: "–", base: 0, cod: 0, bonus: 0 },
  { month: "–", base: 0, cod: 0, bonus: 0 },
  { month: "–", base: 0, cod: 0, bonus: 0 },
  { month: "–", base: 0, cod: 0, bonus: 0 },
];
```

- [ ] **Step 2: Add `getCommissionBreakdown` to paymentsApi**

Locate the `paymentsApi` object in `apps/partner-portal/src/lib/api/payments.ts` (or wherever `paymentsApi` is defined). Add:

```typescript
async getCommissionBreakdown(params: { partner_id: string; year: number; month: number }) {
  const url = `${API_BASE}/v1/partner/commission/breakdown?partner_id=${params.partner_id}&year=${params.year}&month=${params.month}`;
  const res = await authFetch(url);
  if (!res.ok) throw new Error(`Commission breakdown failed: ${res.status}`);
  const json = await res.json();
  return json.data as {
    period: string;
    base_charges_centavos: number;
    cod_remittance_centavos: number;
    bonuses_centavos: number;
    total_centavos: number;
    currency: string;
  };
},
```

Also add `partner_id` to the `Wallet` type if not already present:
```typescript
partner_id: string;
```

- [ ] **Step 3: Verify TypeScript compile**

Run: `cd D:\LogisticOS\apps\partner-portal && npx tsc --noEmit`
Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add apps/partner-portal/src/app/(dashboard)/payouts/page.tsx
git add apps/partner-portal/src/lib/api/payments.ts
git commit -m "feat(partner-portal): replace static commission chart with live breakdown API"
```

---

## PHASE 2

---

### Task 8: Phase 2 Migration

**Files:**
- Create: `services/payments/migrations/0010_create_withdrawal_requests.sql`

- [ ] **Step 1: Write migration**

```sql
-- services/payments/migrations/0010_create_withdrawal_requests.sql
CREATE TYPE payments.withdrawal_status AS ENUM ('pending', 'approved', 'disbursed', 'rejected');

CREATE TABLE payments.withdrawal_requests (
  id                UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id         UUID NOT NULL,
  wallet_id         UUID NOT NULL REFERENCES payments.wallets(id),
  amount_centavos   BIGINT NOT NULL,
  currency          CHAR(3) NOT NULL DEFAULT 'PHP',
  status            payments.withdrawal_status NOT NULL DEFAULT 'pending',
  requested_by      UUID NOT NULL,
  reviewed_by       UUID,
  review_note       TEXT,
  reviewed_at       TIMESTAMPTZ,
  created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX ON payments.withdrawal_requests (wallet_id, status);

ALTER TABLE payments.wallets
  ADD COLUMN reserved_centavos BIGINT NOT NULL DEFAULT 0;
```

- [ ] **Step 2: Apply migration**

Run: `cd D:\LogisticOS\services\payments && cargo sqlx migrate run`
Expected: exits 0.

- [ ] **Step 3: Commit**

```bash
git add services/payments/migrations/0010_create_withdrawal_requests.sql
git commit -m "feat(payments): withdrawal_requests table and wallets.reserved_centavos"
```

---

### Task 9: Update Wallet Entity + WalletSummary

**Files:**
- Modify: `services/payments/src/domain/entities/wallet.rs`
- Modify: `services/payments/src/infrastructure/db/wallet_repo.rs`
- Modify: `services/payments/src/application/commands/mod.rs`
- Modify: `services/payments/src/application/services/wallet_service.rs`

- [ ] **Step 1: Add `reserved_centavos` to Wallet entity**

In `services/payments/src/domain/entities/wallet.rs`, add the field to the `Wallet` struct after `version`:
```rust
pub reserved_centavos: i64,
```

Update `Wallet::new` to set it:
```rust
reserved_centavos: 0,
```

Add two methods to `impl Wallet`:
```rust
/// Reserve an amount for a pending withdrawal. Does not debit balance.
pub fn reserve(&mut self, amount: i64) -> Result<(), &'static str> {
    if self.available_centavos() < amount {
        return Err("Insufficient available balance");
    }
    self.reserved_centavos += amount;
    self.version += 1;
    self.updated_at = Utc::now();
    Ok(())
}

/// Release a reservation (on rejection or cancellation).
pub fn release_reservation(&mut self, amount: i64) {
    self.reserved_centavos = (self.reserved_centavos - amount).max(0);
    self.version += 1;
    self.updated_at = Utc::now();
}

/// Balance available for new withdrawals (balance minus reserved).
pub fn available_centavos(&self) -> i64 {
    self.balance.amount - self.reserved_centavos
}
```

- [ ] **Step 2: Write failing test for reserve**

Add to the `#[cfg(test)]` block in wallet.rs (or create one if absent):
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use logisticos_types::{Currency, TenantId};

    fn make_wallet() -> Wallet {
        let mut w = Wallet::new(TenantId::from_uuid(uuid::Uuid::new_v4()), Currency::PHP);
        w.credit(Money::new(100_000, Currency::PHP)).unwrap();
        w
    }

    #[test]
    fn reserve_reduces_available() {
        let mut w = make_wallet();
        w.reserve(30_000).unwrap();
        assert_eq!(w.available_centavos(), 70_000);
        assert_eq!(w.balance.amount, 100_000); // balance unchanged
    }

    #[test]
    fn reserve_fails_when_insufficient() {
        let mut w = make_wallet();
        assert!(w.reserve(200_000).is_err());
    }

    #[test]
    fn release_reservation_restores_available() {
        let mut w = make_wallet();
        w.reserve(30_000).unwrap();
        w.release_reservation(30_000);
        assert_eq!(w.available_centavos(), 100_000);
    }
}
```

- [ ] **Step 3: Run wallet tests**

Run: `cd D:\LogisticOS\services\payments && cargo test domain::entities::wallet`
Expected: all tests pass.

- [ ] **Step 4: Update WalletRow and SQL in wallet_repo.rs**

In `services/payments/src/infrastructure/db/wallet_repo.rs`:

Add `reserved_centavos: i64` to `WalletRow`.

Update `From<WalletRow> for Wallet` to set `reserved_centavos: r.reserved_centavos`.

In `find_by_tenant`, update the SELECT:
```rust
"SELECT id, tenant_id, balance_cents, currency, version, reserved_centavos, created_at, updated_at
 FROM payments.wallets WHERE tenant_id = $1"
```

In `save_wallet`, update INSERT and ON CONFLICT:
```rust
r#"INSERT INTO payments.wallets (id, tenant_id, balance_cents, currency, version, reserved_centavos, created_at, updated_at)
   VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
   ON CONFLICT (tenant_id) DO UPDATE SET
       balance_cents     = EXCLUDED.balance_cents,
       version           = EXCLUDED.version,
       reserved_centavos = EXCLUDED.reserved_centavos,
       updated_at        = EXCLUDED.updated_at
   WHERE payments.wallets.version = $5 - 1"#
```
And add `.bind(w.reserved_centavos)` before `.bind(w.created_at)`.

Add a new method to `PgWalletRepository` (outside the trait impl):
```rust
pub async fn find_by_id(&self, id: uuid::Uuid) -> anyhow::Result<Option<Wallet>> {
    let row = sqlx::query_as::<_, WalletRow>(
        "SELECT id, tenant_id, balance_cents, currency, version, reserved_centavos, created_at, updated_at
         FROM payments.wallets WHERE id = $1"
    ).bind(id).fetch_optional(&self.pool).await?;
    Ok(row.map(Wallet::from))
}

pub async fn save_wallet_direct(&self, w: &Wallet) -> anyhow::Result<()> {
    self.save_wallet(w).await
}
```

Also add a `find_by_id` method and an `update_reserved` helper to the `WalletRepository` trait:
```rust
async fn find_by_id(&self, id: uuid::Uuid) -> anyhow::Result<Option<Wallet>>;
```

Implement it in `PgWalletRepository` using the query above.

- [ ] **Step 5: Update WalletSummary**

In `services/payments/src/application/commands/mod.rs`, update `WalletSummary`:
```rust
#[derive(Debug, Serialize)]
pub struct WalletSummary {
    pub wallet_id:         Uuid,
    pub balance_centavos:  i64,
    pub reserved_centavos: i64,
    pub available_centavos: i64,
    pub currency:          String,
    pub updated_at:        String,
}
```

Update `WalletService::summary()` in `wallet_service.rs`:
```rust
pub async fn summary(&self, tenant_id: &TenantId) -> AppResult<WalletSummary> {
    let wallet = self.get_or_create(tenant_id).await?;
    Ok(WalletSummary {
        wallet_id:          wallet.id,
        balance_centavos:   wallet.balance.amount,
        reserved_centavos:  wallet.reserved_centavos,
        available_centavos: wallet.available_centavos(),
        currency:           format!("{:?}", wallet.currency),
        updated_at:         wallet.updated_at.to_rfc3339(),
    })
}
```

- [ ] **Step 6: Verify compile + tests**

Run: `cd D:\LogisticOS\services\payments && cargo test`
Expected: all tests pass.

- [ ] **Step 7: Commit**

```bash
git add services/payments/src/domain/entities/wallet.rs
git add services/payments/src/infrastructure/db/wallet_repo.rs
git add services/payments/src/application/commands/mod.rs
git add services/payments/src/application/services/wallet_service.rs
git commit -m "feat(payments): wallet reserved_centavos, available_centavos, updated WalletSummary"
```

---

### Task 10: WithdrawalRequest Entity + Repo + Service

**Files:**
- Create: `services/payments/src/domain/entities/withdrawal_request.rs`
- Create: `services/payments/src/infrastructure/db/withdrawal_request_repo.rs`
- Create: `services/payments/src/application/services/withdrawal_service.rs`
- Modify: `services/payments/src/domain/entities/mod.rs`
- Modify: `services/payments/src/infrastructure/db/mod.rs`
- Modify: `services/payments/src/application/services/mod.rs`
- Modify: `services/payments/src/application/commands/mod.rs`

- [ ] **Step 1: Write withdrawal request entity**

Create `services/payments/src/domain/entities/withdrawal_request.rs`:

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WithdrawalStatus {
    Pending,
    Approved,
    Disbursed,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WithdrawalRequest {
    pub id:              Uuid,
    pub tenant_id:       Uuid,
    pub wallet_id:       Uuid,
    pub amount_centavos: i64,
    pub currency:        String,
    pub status:          WithdrawalStatus,
    pub requested_by:    Uuid,
    pub reviewed_by:     Option<Uuid>,
    pub review_note:     Option<String>,
    pub reviewed_at:     Option<DateTime<Utc>>,
    pub created_at:      DateTime<Utc>,
    pub updated_at:      DateTime<Utc>,
}

impl WithdrawalRequest {
    pub fn new(tenant_id: Uuid, wallet_id: Uuid, amount_centavos: i64, requested_by: Uuid) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            wallet_id,
            amount_centavos,
            currency: "PHP".into(),
            status: WithdrawalStatus::Pending,
            requested_by,
            reviewed_by: None,
            review_note: None,
            reviewed_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn approve(&mut self, reviewed_by: Uuid) -> Result<(), &'static str> {
        if self.status != WithdrawalStatus::Pending {
            return Err("Only pending requests can be approved");
        }
        self.status = WithdrawalStatus::Approved;
        self.reviewed_by = Some(reviewed_by);
        self.reviewed_at = Some(Utc::now());
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn disburse(&mut self, reviewed_by: Uuid) -> Result<(), &'static str> {
        if self.status != WithdrawalStatus::Approved {
            return Err("Only approved requests can be disbursed");
        }
        self.status = WithdrawalStatus::Disbursed;
        self.reviewed_by = Some(reviewed_by);
        self.reviewed_at = Some(Utc::now());
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn reject(&mut self, reviewed_by: Uuid, note: String) -> Result<(), &'static str> {
        if !matches!(self.status, WithdrawalStatus::Pending | WithdrawalStatus::Approved) {
            return Err("Only pending or approved requests can be rejected");
        }
        self.status = WithdrawalStatus::Rejected;
        self.reviewed_by = Some(reviewed_by);
        self.review_note = Some(note);
        self.reviewed_at = Some(Utc::now());
        self.updated_at = Utc::now();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req() -> WithdrawalRequest {
        WithdrawalRequest::new(Uuid::new_v4(), Uuid::new_v4(), 50_000, Uuid::new_v4())
    }

    #[test]
    fn approve_transitions_to_approved() {
        let mut r = req();
        r.approve(Uuid::new_v4()).unwrap();
        assert_eq!(r.status, WithdrawalStatus::Approved);
    }

    #[test]
    fn disburse_requires_approved() {
        let mut r = req();
        assert!(r.disburse(Uuid::new_v4()).is_err());
        r.approve(Uuid::new_v4()).unwrap();
        r.disburse(Uuid::new_v4()).unwrap();
        assert_eq!(r.status, WithdrawalStatus::Disbursed);
    }

    #[test]
    fn reject_from_pending() {
        let mut r = req();
        r.reject(Uuid::new_v4(), "Policy".into()).unwrap();
        assert_eq!(r.status, WithdrawalStatus::Rejected);
        assert_eq!(r.review_note.as_deref(), Some("Policy"));
    }
}
```

- [ ] **Step 2: Run entity tests**

Run: `cd D:\LogisticOS\services\payments && cargo test domain::entities::withdrawal_request`
Expected: 3 tests pass.

- [ ] **Step 3: Write the repo**

Create `services/payments/src/infrastructure/db/withdrawal_request_repo.rs`:

```rust
use sqlx::PgPool;
use uuid::Uuid;
use crate::domain::entities::{WithdrawalRequest, WithdrawalStatus};

pub struct PgWithdrawalRequestRepository { pool: PgPool }
impl PgWithdrawalRequestRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[derive(sqlx::FromRow)]
struct RequestRow {
    id:              Uuid,
    tenant_id:       Uuid,
    wallet_id:       Uuid,
    amount_centavos: i64,
    currency:        String,
    status:          String,
    requested_by:    Uuid,
    reviewed_by:     Option<Uuid>,
    review_note:     Option<String>,
    reviewed_at:     Option<chrono::DateTime<chrono::Utc>>,
    created_at:      chrono::DateTime<chrono::Utc>,
    updated_at:      chrono::DateTime<chrono::Utc>,
}

fn parse_status(s: &str) -> WithdrawalStatus {
    match s {
        "approved"  => WithdrawalStatus::Approved,
        "disbursed" => WithdrawalStatus::Disbursed,
        "rejected"  => WithdrawalStatus::Rejected,
        _           => WithdrawalStatus::Pending,
    }
}
fn status_str(s: WithdrawalStatus) -> &'static str {
    match s {
        WithdrawalStatus::Pending   => "pending",
        WithdrawalStatus::Approved  => "approved",
        WithdrawalStatus::Disbursed => "disbursed",
        WithdrawalStatus::Rejected  => "rejected",
    }
}

impl From<RequestRow> for WithdrawalRequest {
    fn from(r: RequestRow) -> Self {
        WithdrawalRequest {
            id: r.id, tenant_id: r.tenant_id, wallet_id: r.wallet_id,
            amount_centavos: r.amount_centavos, currency: r.currency,
            status: parse_status(&r.status), requested_by: r.requested_by,
            reviewed_by: r.reviewed_by, review_note: r.review_note,
            reviewed_at: r.reviewed_at, created_at: r.created_at, updated_at: r.updated_at,
        }
    }
}

const SELECT: &str = "SELECT id, tenant_id, wallet_id, amount_centavos, currency, status,
    requested_by, reviewed_by, review_note, reviewed_at, created_at, updated_at
    FROM payments.withdrawal_requests";

impl PgWithdrawalRequestRepository {
    pub async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<WithdrawalRequest>> {
        let row = sqlx::query_as::<_, RequestRow>(&format!("{SELECT} WHERE id = $1"))
            .bind(id).fetch_optional(&self.pool).await?;
        Ok(row.map(WithdrawalRequest::from))
    }

    pub async fn list_by_status(&self, tenant_id: Uuid, status: WithdrawalStatus) -> anyhow::Result<Vec<WithdrawalRequest>> {
        let rows = sqlx::query_as::<_, RequestRow>(
            &format!("{SELECT} WHERE tenant_id = $1 AND status = $2 ORDER BY created_at DESC")
        ).bind(tenant_id).bind(status_str(status)).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(WithdrawalRequest::from).collect())
    }

    pub async fn insert(&self, r: &WithdrawalRequest) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO payments.withdrawal_requests
             (id, tenant_id, wallet_id, amount_centavos, currency, status,
              requested_by, reviewed_by, review_note, reviewed_at, created_at, updated_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)"
        )
        .bind(r.id).bind(r.tenant_id).bind(r.wallet_id).bind(r.amount_centavos)
        .bind(&r.currency).bind(status_str(r.status)).bind(r.requested_by)
        .bind(r.reviewed_by).bind(r.review_note.as_deref())
        .bind(r.reviewed_at).bind(r.created_at).bind(r.updated_at)
        .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn update(&self, r: &WithdrawalRequest) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE payments.withdrawal_requests SET
             status = $2, reviewed_by = $3, review_note = $4, reviewed_at = $5, updated_at = $6
             WHERE id = $1"
        )
        .bind(r.id).bind(status_str(r.status)).bind(r.reviewed_by)
        .bind(r.review_note.as_deref()).bind(r.reviewed_at).bind(r.updated_at)
        .execute(&self.pool).await?;
        Ok(())
    }
}
```

- [ ] **Step 4: Write WithdrawalService**

Create `services/payments/src/application/services/withdrawal_service.rs`:

```rust
use std::sync::Arc;
use uuid::Uuid;
use logisticos_errors::{AppError, AppResult};
use logisticos_events::producer::KafkaProducer;
use logisticos_types::{Currency, Money, TenantId};
use crate::{
    domain::entities::{TransactionType, Wallet, WalletTransaction, WithdrawalRequest, WithdrawalStatus},
    domain::repositories::WalletRepository,
    domain::value_objects::MIN_WITHDRAWAL_CENTS,
    infrastructure::db::withdrawal_request_repo::PgWithdrawalRequestRepository,
};

pub struct WithdrawalService {
    wallet_repo:      Arc<dyn WalletRepository>,
    withdrawal_repo:  Arc<PgWithdrawalRequestRepository>,
    kafka:            Arc<KafkaProducer>,
}

impl WithdrawalService {
    pub fn new(
        wallet_repo:     Arc<dyn WalletRepository>,
        withdrawal_repo: Arc<PgWithdrawalRequestRepository>,
        kafka:           Arc<KafkaProducer>,
    ) -> Self {
        Self { wallet_repo, withdrawal_repo, kafka }
    }

    /// Create a pending withdrawal request. Reserves amount in wallet.
    pub async fn request(
        &self,
        tenant_id: &TenantId,
        amount_centavos: i64,
        requested_by: Uuid,
    ) -> AppResult<WithdrawalRequest> {
        if amount_centavos < MIN_WITHDRAWAL_CENTS {
            return Err(AppError::BusinessRule(format!(
                "Minimum withdrawal is ₱{:.2}", MIN_WITHDRAWAL_CENTS as f64 / 100.0
            )));
        }
        let mut wallet = self.wallet_repo.find_by_tenant(tenant_id).await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::BusinessRule("Wallet not found".into()))?;

        wallet.reserve(amount_centavos)
            .map_err(|e| AppError::BusinessRule(e.to_string()))?;
        self.wallet_repo.save_wallet(&wallet).await.map_err(AppError::Internal)?;

        let req = WithdrawalRequest::new(tenant_id.inner(), wallet.id, amount_centavos, requested_by);
        self.withdrawal_repo.insert(&req).await.map_err(AppError::Internal)?;
        Ok(req)
    }

    /// Finance approves (status-only, no ledger change).
    pub async fn approve(&self, id: Uuid, reviewed_by: Uuid) -> AppResult<WithdrawalRequest> {
        let mut req = self.find_or_error(id).await?;
        req.approve(reviewed_by).map_err(|e| AppError::BusinessRule(e.to_string()))?;
        self.withdrawal_repo.update(&req).await.map_err(AppError::Internal)?;
        Ok(req)
    }

    /// Finance disburses — debits wallet, emits Kafka event.
    pub async fn disburse(&self, id: Uuid, reviewed_by: Uuid, tenant_id: &TenantId) -> AppResult<WithdrawalRequest> {
        let mut req = self.find_or_error(id).await?;
        req.disburse(reviewed_by).map_err(|e| AppError::BusinessRule(e.to_string()))?;
        self.withdrawal_repo.update(&req).await.map_err(AppError::Internal)?;

        // Debit wallet balance and clear reservation
        let mut wallet = self.wallet_repo.find_by_id(req.wallet_id).await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::Internal(anyhow::anyhow!("Wallet {} not found", req.wallet_id)))?;

        wallet.debit(Money::new(req.amount_centavos, Currency::PHP))
            .map_err(|e| AppError::BusinessRule(e.to_string()))?;
        wallet.release_reservation(req.amount_centavos);
        self.wallet_repo.save_wallet(&wallet).await.map_err(AppError::Internal)?;

        let tx = WalletTransaction {
            id: uuid::Uuid::new_v4(),
            wallet_id: wallet.id,
            tenant_id: tenant_id.clone(),
            transaction_type: TransactionType::Withdrawal,
            amount: Money::new(req.amount_centavos, Currency::PHP),
            reference_id: req.id,
            description: format!("Withdrawal disbursed: ₱{:.2}", req.amount_centavos as f64 / 100.0),
            created_at: chrono::Utc::now(),
        };
        self.wallet_repo.record_transaction(&tx).await.map_err(AppError::Internal)?;

        let event_payload = serde_json::json!({
            "withdrawal_request_id": req.id,
            "wallet_id":             req.wallet_id,
            "amount_centavos":       req.amount_centavos,
            "tenant_id":             tenant_id.inner(),
        });
        let _ = self.kafka.publish("wallet.withdrawal_disbursed", &event_payload.to_string()).await;

        Ok(req)
    }

    /// Finance rejects — releases reservation.
    pub async fn reject(&self, id: Uuid, reviewed_by: Uuid, note: String, tenant_id: &TenantId) -> AppResult<WithdrawalRequest> {
        let mut req = self.find_or_error(id).await?;
        req.reject(reviewed_by, note).map_err(|e| AppError::BusinessRule(e.to_string()))?;
        self.withdrawal_repo.update(&req).await.map_err(AppError::Internal)?;

        // Release reservation
        let mut wallet = self.wallet_repo.find_by_id(req.wallet_id).await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::Internal(anyhow::anyhow!("Wallet {} not found", req.wallet_id)))?;
        wallet.release_reservation(req.amount_centavos);
        self.wallet_repo.save_wallet(&wallet).await.map_err(AppError::Internal)?;

        let event_payload = serde_json::json!({
            "withdrawal_request_id": req.id,
            "tenant_id":             tenant_id.inner(),
        });
        let _ = self.kafka.publish("wallet.withdrawal_rejected", &event_payload.to_string()).await;

        Ok(req)
    }

    pub async fn list_pending(&self, tenant_id: Uuid) -> AppResult<Vec<WithdrawalRequest>> {
        self.withdrawal_repo.list_by_status(tenant_id, WithdrawalStatus::Pending)
            .await.map_err(AppError::Internal)
    }

    async fn find_or_error(&self, id: Uuid) -> AppResult<WithdrawalRequest> {
        self.withdrawal_repo.find_by_id(id).await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound(format!("Withdrawal request {id} not found")))
    }
}
```

- [ ] **Step 5: Register in mods**

In `services/payments/src/domain/entities/mod.rs`, append:
```rust
pub mod withdrawal_request;
pub use withdrawal_request::{WithdrawalRequest, WithdrawalStatus};
```

In `services/payments/src/infrastructure/db/mod.rs`, append:
```rust
pub mod withdrawal_request_repo;
pub use withdrawal_request_repo::PgWithdrawalRequestRepository;
```

In `services/payments/src/application/services/mod.rs`, append:
```rust
pub mod withdrawal_service;
pub use withdrawal_service::WithdrawalService;
```

- [ ] **Step 6: Add `find_by_id` to WalletRepository trait**

In `services/payments/src/domain/repositories/mod.rs`, add to `WalletRepository`:
```rust
async fn find_by_id(&self, id: uuid::Uuid) -> anyhow::Result<Option<Wallet>>;
```

Implement in `PgWalletRepository` (wallet_repo.rs):
```rust
async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<Wallet>> {
    let row = sqlx::query_as::<_, WalletRow>(
        "SELECT id, tenant_id, balance_cents, currency, version, reserved_centavos, created_at, updated_at
         FROM payments.wallets WHERE id = $1"
    ).bind(id).fetch_optional(&self.pool).await?;
    Ok(row.map(Wallet::from))
}
```

- [ ] **Step 7: Verify compile + tests**

Run: `cd D:\LogisticOS\services\payments && cargo test`
Expected: all tests pass.

- [ ] **Step 8: Commit**

```bash
git add services/payments/src/domain/entities/withdrawal_request.rs
git add services/payments/src/domain/entities/mod.rs
git add services/payments/src/infrastructure/db/withdrawal_request_repo.rs
git add services/payments/src/infrastructure/db/mod.rs
git add services/payments/src/application/services/withdrawal_service.rs
git add services/payments/src/application/services/mod.rs
git add services/payments/src/domain/repositories/mod.rs
git commit -m "feat(payments): WithdrawalRequest entity, repo, and WithdrawalService"
```

---

### Task 11: Withdrawal HTTP Handlers + Rework Wallet Handler

**Files:**
- Create: `services/payments/src/api/http/withdrawal_requests.rs`
- Modify: `services/payments/src/api/http/wallet.rs`
- Modify: `services/payments/src/api/http/mod.rs`
- Modify: `services/payments/src/bootstrap.rs`

- [ ] **Step 1: Write withdrawal handlers**

Create `services/payments/src/api/http/withdrawal_requests.rs`:

```rust
use axum::{extract::{Path, Query, State}, Json, http::StatusCode};
use std::sync::Arc;
use serde::Deserialize;
use uuid::Uuid;
use logisticos_auth::middleware::AuthClaims;
use logisticos_auth::require_permission;
use logisticos_errors::AppError;
use logisticos_types::TenantId;
use crate::api::http::AppState;

#[derive(Deserialize)]
pub struct ListParams { status: Option<String> }

/// GET /v1/admin/withdrawal-requests
pub async fn list_withdrawal_requests(
    AuthClaims(claims): AuthClaims,
    Query(params): Query<ListParams>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::BILLING_ADMIN);
    let requests = state.withdrawal_service.list_pending(claims.tenant_id).await?;
    Ok(Json(serde_json::json!({ "data": requests })))
}

/// POST /v1/admin/withdrawal-requests/:id/approve
pub async fn approve_withdrawal(
    AuthClaims(claims): AuthClaims,
    Path(id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::BILLING_ADMIN);
    let req = state.withdrawal_service.approve(id, claims.user_id).await?;
    Ok(Json(serde_json::json!({ "data": req })))
}

/// POST /v1/admin/withdrawal-requests/:id/disburse
pub async fn disburse_withdrawal(
    AuthClaims(claims): AuthClaims,
    Path(id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::BILLING_ADMIN);
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let req = state.withdrawal_service.disburse(id, claims.user_id, &tenant_id).await?;
    Ok(Json(serde_json::json!({ "data": req })))
}

#[derive(Deserialize)]
pub struct RejectBody { reason: String }

/// POST /v1/admin/withdrawal-requests/:id/reject
pub async fn reject_withdrawal(
    AuthClaims(claims): AuthClaims,
    Path(id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<RejectBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::BILLING_ADMIN);
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let req = state.withdrawal_service.reject(id, claims.user_id, body.reason, &tenant_id).await?;
    Ok(Json(serde_json::json!({ "data": req })))
}
```

- [ ] **Step 2: Rework wallet `request_withdrawal` handler**

Replace the `request_withdrawal` function in `services/payments/src/api/http/wallet.rs` with:

```rust
pub async fn request_withdrawal(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
    Json(cmd): Json<RequestWithdrawalCommand>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::BILLING_MANAGE);
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let req = state.withdrawal_service
        .request(&tenant_id, cmd.amount_cents, claims.user_id).await?;
    let summary = state.wallet_service.summary(&tenant_id).await?;
    Ok(Json(serde_json::json!({
        "withdrawal_request_id": req.id,
        "status":                "pending",
        "reserved_centavos":     summary.reserved_centavos,
        "available_centavos":    summary.available_centavos,
    })))
}
```

Remove the `ReconcileCodCommand` import for `RequestWithdrawalCommand` from `crate::application::commands` — keep only what's still needed.

Add `use logisticos_types::TenantId;` if not present.

- [ ] **Step 3: Register in AppState and router**

In `services/payments/src/api/http/mod.rs`:

Add `pub mod withdrawal_requests;` at top.

Add to `AppState`:
```rust
pub withdrawal_service: Arc<crate::application::services::WithdrawalService>,
```

In `protected_router`, append:
```rust
.route("/admin/withdrawal-requests",             get(withdrawal_requests::list_withdrawal_requests))
.route("/admin/withdrawal-requests/:id/approve", post(withdrawal_requests::approve_withdrawal))
.route("/admin/withdrawal-requests/:id/disburse",post(withdrawal_requests::disburse_withdrawal))
.route("/admin/withdrawal-requests/:id/reject",  post(withdrawal_requests::reject_withdrawal))
```

- [ ] **Step 4: Wire in bootstrap**

In `services/payments/src/bootstrap.rs`:

```rust
let withdrawal_repo = Arc::new(PgWithdrawalRequestRepository::new(pool.clone()));
let withdrawal_service = Arc::new(WithdrawalService::new(
    Arc::clone(&wallet_repo) as _,
    Arc::clone(&withdrawal_repo),
    Arc::clone(&kafka),
));
```

Add to imports and `AppState`:
```rust
withdrawal_service: Arc::clone(&withdrawal_service),
```

- [ ] **Step 5: Verify compile + tests**

Run: `cd D:\LogisticOS\services\payments && cargo test`
Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add services/payments/src/api/http/withdrawal_requests.rs
git add services/payments/src/api/http/wallet.rs
git add services/payments/src/api/http/mod.rs
git add services/payments/src/bootstrap.rs
git commit -m "feat(payments): withdrawal ops flow — admin approve/disburse/reject endpoints"
```

---

### Task 12: Invoice PDF — Cargo.toml + PdfRenderer + Template + Handler

**Files:**
- Modify: `services/payments/Cargo.toml`
- Create: `services/payments/src/application/services/pdf_renderer.rs`
- Create: `services/payments/templates/invoice.html`
- Create: `services/payments/src/api/http/invoice_pdf.rs`
- Modify: `services/payments/src/application/services/mod.rs`
- Modify: `services/payments/src/api/http/mod.rs`
- Modify: `services/payments/src/bootstrap.rs`

- [ ] **Step 1: Add dependencies to Cargo.toml**

In `services/payments/Cargo.toml`, add under `[dependencies]`:

```toml
chromiumoxide = { version = "0.6", features = ["tokio-runtime"] }
tera = "1"
```

- [ ] **Step 2: Write PdfRenderer**

Create `services/payments/src/application/services/pdf_renderer.rs`:

```rust
//! Renders invoice HTML to PDF bytes via a shared headless Chrome browser.
//! One Browser instance is held in AppState; each request gets one tab.

use std::sync::Arc;
use anyhow::Context;
use chromiumoxide::browser::{Browser, BrowserConfig};
use tera::Tera;
use tokio::sync::Mutex;

pub struct PdfRenderer {
    browser: Arc<Mutex<Browser>>,
    tera:    Tera,
}

impl PdfRenderer {
    pub async fn new(template_dir: &str) -> anyhow::Result<Self> {
        let config = BrowserConfig::builder()
            .no_sandbox()
            .build()
            .map_err(|e| anyhow::anyhow!("BrowserConfig error: {e}"))?;
        let (browser, mut handler) = Browser::launch(config).await
            .context("Failed to launch Chromium")?;

        tokio::spawn(async move {
            loop {
                if handler.next().await.is_none() { break; }
            }
        });

        let glob = format!("{template_dir}/**/*.html");
        let tera = Tera::new(&glob).context("Failed to load Tera templates")?;

        Ok(Self {
            browser: Arc::new(Mutex::new(browser)),
            tera,
        })
    }

    pub async fn render_invoice(
        &self,
        context: &tera::Context,
    ) -> anyhow::Result<Vec<u8>> {
        let html = self.tera.render("invoice.html", context)
            .context("Tera render failed")?;

        let browser = self.browser.lock().await;
        let page = browser.new_page("about:blank").await
            .context("Failed to open Chrome tab")?;

        page.set_content(html).await.context("Failed to set page content")?;

        let pdf_opts = chromiumoxide::page::PrintToPdfParams::default();
        let pdf_bytes = page.pdf(pdf_opts).await.context("Failed to print PDF")?;
        page.close().await.ok();

        Ok(pdf_bytes)
    }
}
```

- [ ] **Step 3: Write the invoice HTML template**

Create `services/payments/templates/invoice.html`:

```html
<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<style>
  body { font-family: Arial, sans-serif; font-size: 12px; color: #1a1a1a; margin: 0; padding: 40px; }
  .header { display: flex; justify-content: space-between; margin-bottom: 32px; }
  .company { font-size: 20px; font-weight: bold; }
  .invoice-meta { text-align: right; }
  .invoice-meta h2 { font-size: 24px; font-weight: bold; color: #2563eb; margin: 0 0 4px; }
  .section { margin-bottom: 24px; }
  .section h3 { font-size: 11px; text-transform: uppercase; color: #6b7280; margin-bottom: 4px; }
  table { width: 100%; border-collapse: collapse; }
  th { background: #f3f4f6; text-align: left; padding: 8px; font-size: 11px; text-transform: uppercase; color: #6b7280; }
  td { padding: 8px; border-bottom: 1px solid #e5e7eb; }
  .totals { margin-left: auto; width: 280px; margin-top: 16px; }
  .totals tr td:first-child { color: #6b7280; }
  .totals tr td:last-child { text-align: right; font-weight: bold; }
  .total-row td { font-size: 14px; font-weight: bold; border-top: 2px solid #1a1a1a; padding-top: 8px; }
  .footer { margin-top: 40px; padding-top: 16px; border-top: 1px solid #e5e7eb; color: #9ca3af; font-size: 10px; }
  .status-badge { display: inline-block; padding: 2px 8px; border-radius: 4px; font-size: 11px; font-weight: bold; }
  .status-issued   { background: #fef3c7; color: #92400e; }
  .status-paid     { background: #d1fae5; color: #065f46; }
  .status-overdue  { background: #fee2e2; color: #991b1b; }
</style>
</head>
<body>
<div class="header">
  <div>
    <div class="company">{{ tenant_name }}</div>
    <div style="color:#6b7280; margin-top:4px">{{ tenant_address | default(value="") }}</div>
  </div>
  <div class="invoice-meta">
    <h2>INVOICE</h2>
    <div><strong>{{ invoice_number }}</strong></div>
    <div style="color:#6b7280">Issued: {{ issued_at }}</div>
    <div style="color:#6b7280">Due: {{ due_at }}</div>
    <div style="margin-top:8px">
      <span class="status-badge status-{{ status }}">{{ status | upper }}</span>
    </div>
  </div>
</div>

<div class="section">
  <h3>Bill To</h3>
  <div><strong>Merchant ID:</strong> {{ merchant_id }}</div>
  {% if billing_email %}<div>{{ billing_email }}</div>{% endif %}
</div>

<div class="section">
  <h3>Billing Period</h3>
  <div>{{ period_start }} – {{ period_end }}</div>
</div>

<div class="section">
  <h3>Charges</h3>
  <table>
    <thead>
      <tr>
        <th>AWB</th>
        <th>Description</th>
        <th style="text-align:right">Qty</th>
        <th style="text-align:right">Unit Price</th>
        <th style="text-align:right">Amount</th>
      </tr>
    </thead>
    <tbody>
      {% for item in line_items %}
      <tr>
        <td>{{ item.awb | default(value="—") }}</td>
        <td>{{ item.description }}</td>
        <td style="text-align:right">{{ item.quantity }}</td>
        <td style="text-align:right">₱{{ item.unit_price_php }}</td>
        <td style="text-align:right">₱{{ item.net_php }}</td>
      </tr>
      {% endfor %}
    </tbody>
  </table>
  {% if adjustments %}
  <div style="margin-top:12px; font-size:11px; color:#6b7280">Adjustments:</div>
  <table>
    {% for adj in adjustments %}
    <tr>
      <td>{{ adj.awb | default(value="—") }}</td>
      <td>{{ adj.reason }}</td>
      <td style="text-align:right">₱{{ adj.amount_php }}</td>
    </tr>
    {% endfor %}
  </table>
  {% endif %}
  <table class="totals">
    <tr><td>Subtotal</td><td>₱{{ subtotal_php }}</td></tr>
    <tr><td>VAT (12%)</td><td>₱{{ vat_php }}</td></tr>
    <tr class="total-row"><td>Total Due</td><td>₱{{ total_php }}</td></tr>
  </table>
</div>

<div class="footer">
  This is a system-generated invoice. For disputes, contact your account manager within {{ payment_terms_days }} days of issue date.
</div>
</body>
</html>
```

- [ ] **Step 4: Write invoice PDF handler**

Create `services/payments/src/api/http/invoice_pdf.rs`:

```rust
use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, Response, StatusCode},
    response::IntoResponse,
};
use std::sync::Arc;
use logisticos_auth::middleware::AuthClaims;
use logisticos_auth::require_permission;
use logisticos_errors::AppError;
use logisticos_types::{InvoiceId, TenantId};
use tera::Context;
use crate::api::http::AppState;

/// GET /v1/invoices/:id/pdf
pub async fn download_invoice_pdf(
    AuthClaims(claims): AuthClaims,
    Path(id): Path<uuid::Uuid>,
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::BILLING_VIEW);

    let invoice_id = InvoiceId::from_uuid(id);
    let invoice = state.invoice_service
        .get_invoice(&TenantId::from_uuid(claims.tenant_id), &invoice_id).await?
        .ok_or_else(|| AppError::NotFound(format!("Invoice {id} not found")))?;

    let mut ctx = Context::new();
    ctx.insert("tenant_name",       &claims.tenant_id.to_string()); // replace with tenant name when available
    ctx.insert("invoice_number",    &invoice.invoice_number.to_string());
    ctx.insert("merchant_id",       &invoice.merchant_id.inner().to_string());
    ctx.insert("status",            &format!("{:?}", invoice.status).to_lowercase());
    ctx.insert("issued_at",         &invoice.issued_at.format("%Y-%m-%d").to_string());
    ctx.insert("due_at",            &invoice.due_at.format("%Y-%m-%d").to_string());
    ctx.insert("period_start",      &invoice.billing_period.start.to_string());
    ctx.insert("period_end",        &invoice.billing_period.end.to_string());
    ctx.insert("payment_terms_days", &15);

    let line_items: Vec<serde_json::Value> = invoice.line_items.iter().map(|i| {
        serde_json::json!({
            "awb":          i.awb.as_ref().map(|a| a.as_str().to_string()),
            "description":  i.description,
            "quantity":     i.quantity,
            "unit_price_php": format!("{:.2}", i.unit_price.amount as f64 / 100.0),
            "net_php":        format!("{:.2}", i.net().amount as f64 / 100.0),
        })
    }).collect();
    ctx.insert("line_items", &line_items);

    let adjustments: Vec<serde_json::Value> = invoice.adjustments.iter().map(|a| {
        serde_json::json!({
            "awb":        a.awb.as_ref().map(|x| x.as_str().to_string()),
            "reason":     a.reason,
            "amount_php": format!("{:.2}", a.amount.amount as f64 / 100.0),
        })
    }).collect();
    ctx.insert("adjustments", &adjustments);
    ctx.insert("subtotal_php", &format!("{:.2}", invoice.subtotal().amount as f64 / 100.0));
    ctx.insert("vat_php",      &format!("{:.2}", invoice.vat_amount().amount as f64 / 100.0));
    ctx.insert("total_php",    &format!("{:.2}", invoice.total_due().amount as f64 / 100.0));

    let renderer = state.pdf_renderer.as_ref()
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("PDF renderer not initialised")))?;

    let pdf_bytes = renderer.render_invoice(&ctx).await
        .map_err(AppError::Internal)?;

    let filename = format!("{}.pdf", invoice.invoice_number);
    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/pdf")
        .header(header::CONTENT_DISPOSITION, format!("attachment; filename=\"{filename}\""))
        .body(Body::from(pdf_bytes))
        .map_err(|e| AppError::Internal(anyhow::anyhow!("{e}")))?;

    Ok(response)
}
```

Note: `invoice_service.get_invoice` needs to exist. Add a thin wrapper to `InvoiceService` if it doesn't:
```rust
pub async fn get_invoice(&self, _tenant_id: &TenantId, id: &InvoiceId) -> AppResult<Option<Invoice>> {
    self.invoice_repo.find_by_id(id).await.map_err(AppError::Internal)
}
```

- [ ] **Step 5: Register in AppState and router**

In `services/payments/src/api/http/mod.rs`:

Add `pub mod invoice_pdf;` at top.

Add to `AppState`:
```rust
pub pdf_renderer: Option<Arc<crate::application::services::pdf_renderer::PdfRenderer>>,
```

In `protected_router`, add route:
```rust
.route("/invoices/:id/pdf", get(invoice_pdf::download_invoice_pdf))
```

In `services/payments/src/application/services/mod.rs`, append:
```rust
pub mod pdf_renderer;
pub use pdf_renderer::PdfRenderer;
```

- [ ] **Step 6: Wire PdfRenderer in bootstrap**

In `services/payments/src/bootstrap.rs`, after the database repos are set up, add:

```rust
let templates_dir = std::env::var("PAYMENTS_TEMPLATES_DIR")
    .unwrap_or_else(|_| "./templates".into());
let pdf_renderer = match crate::application::services::PdfRenderer::new(&templates_dir).await {
    Ok(r) => {
        tracing::info!("PDF renderer initialised");
        Some(Arc::new(r))
    }
    Err(e) => {
        tracing::warn!(err = %e, "PDF renderer failed to initialise — /invoices/:id/pdf will return 503");
        None
    }
};
```

Add to `AppState`:
```rust
pdf_renderer,
```

- [ ] **Step 7: Verify compile**

Run: `cd D:\LogisticOS\services\payments && cargo check`
Expected: no errors (Chrome not required to be running for `cargo check`).

- [ ] **Step 8: Run full test suite**

Run: `cd D:\LogisticOS\services\payments && cargo test`
Expected: all tests pass.

- [ ] **Step 9: Commit**

```bash
git add services/payments/Cargo.toml
git add services/payments/src/application/services/pdf_renderer.rs
git add services/payments/templates/invoice.html
git add services/payments/src/api/http/invoice_pdf.rs
git add services/payments/src/application/services/mod.rs
git add services/payments/src/api/http/mod.rs
git add services/payments/src/bootstrap.rs
git commit -m "feat(payments): invoice PDF download via headless Chrome — GET /v1/invoices/:id/pdf"
```

---

## Self-Review Checklist

**Spec coverage:**
- [x] Invoice PDF — Task 12
- [x] Withdrawal ops flow — Tasks 8–11
- [x] Commission breakdown — Task 5
- [x] Merchant billing account CRUD — Tasks 2–4
- [x] Admin billing run trigger — Task 6
- [x] Partner portal chart wired to live data — Task 7

**Type consistency:**
- `WithdrawalStatus` used consistently across entity, repo, service, handler
- `WalletSummary` fields renamed from `balance_cents` → `balance_centavos` — verify callers of `wallet_service.summary()` in `get_wallet` handler use the updated field names
- `CommissionBreakdown` struct in query matches JSON field names referenced in TypeScript

**Known call to verify:** `get_wallet` handler in `wallet.rs` returns `summary` directly as JSON — after `WalletSummary` field rename, confirm the partner portal's `Wallet` TypeScript type maps `balance_centavos` → `balance_php` correctly (divide by 100).
