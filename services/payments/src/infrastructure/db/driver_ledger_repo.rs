//! PostgreSQL implementation of `DriverLedgerRepository`.
//!
//! Schema lives in `payments.driver_ledgers` and `payments.driver_ledger_entries`
//! (added in migration 0011_create_driver_ledgers.sql).
//!
//! # Optimistic locking
//!
//! Every `save()` runs:
//! ```sql
//! UPDATE driver_ledgers SET ... WHERE id = $1 AND version = $expected_version
//! ```
//! If the UPDATE touches 0 rows it means another process already incremented
//! the version — `save()` returns `Err` with a descriptive message so the
//! caller can retry by re-loading the ledger.
//!
//! New entries are inserted with `ON CONFLICT (id) DO NOTHING` so replaying
//! the same Kafka event is always safe.

use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;
use logisticos_types::TenantId;

use crate::domain::{
    entities::{DriverLedger, LedgerEntry, LedgerStatus, EntryKind},
    repositories::DriverLedgerRepository,
};

pub struct PgDriverLedgerRepository {
    pool: PgPool,
}

impl PgDriverLedgerRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

// ── Row structs ────────────────────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct LedgerRow {
    id:            Uuid,
    tenant_id:     Uuid,
    driver_id:     Uuid,
    shift_id:      Option<Uuid>,
    status:        String,
    balance_cents: i64,
    version:       i64,
    created_at:    chrono::DateTime<chrono::Utc>,
    updated_at:    chrono::DateTime<chrono::Utc>,
}

#[derive(sqlx::FromRow)]
struct EntryRow {
    id:           Uuid,
    ledger_id:    Uuid,
    kind:         String,
    amount_cents: i64,
    shipment_id:  Option<Uuid>,
    reference:    Option<String>,
    created_at:   chrono::DateTime<chrono::Utc>,
}

fn parse_status(s: &str) -> LedgerStatus {
    match s {
        "reconciled" => LedgerStatus::Reconciled,
        _            => LedgerStatus::Open,
    }
}

fn status_str(s: LedgerStatus) -> &'static str {
    match s {
        LedgerStatus::Open       => "open",
        LedgerStatus::Reconciled => "reconciled",
    }
}

fn parse_kind(s: &str) -> EntryKind {
    match s {
        "pickup_debit" => EntryKind::PickupDebit,
        "cod_debit"    => EntryKind::CodDebit,
        _              => EntryKind::Remittance,
    }
}

fn kind_str(k: EntryKind) -> &'static str {
    match k {
        EntryKind::PickupDebit => "pickup_debit",
        EntryKind::CodDebit    => "cod_debit",
        EntryKind::Remittance  => "remittance",
    }
}

fn row_to_ledger(row: LedgerRow, entries: Vec<LedgerEntry>) -> DriverLedger {
    DriverLedger {
        id:            row.id,
        tenant_id:     row.tenant_id,
        driver_id:     row.driver_id,
        shift_id:      row.shift_id,
        status:        parse_status(&row.status),
        balance_cents: row.balance_cents,
        version:       row.version,
        entries,
        created_at:    row.created_at,
        updated_at:    row.updated_at,
    }
}

fn entry_row_to_entry(r: EntryRow) -> LedgerEntry {
    LedgerEntry {
        id:           r.id,
        ledger_id:    r.ledger_id,
        kind:         parse_kind(&r.kind),
        amount_cents: r.amount_cents,
        shipment_id:  r.shipment_id,
        reference:    r.reference,
        created_at:   r.created_at,
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────────

async fn load_entries(pool: &PgPool, ledger_id: Uuid) -> anyhow::Result<Vec<LedgerEntry>> {
    let rows = sqlx::query_as::<_, EntryRow>(
        "SELECT id, ledger_id, kind, amount_cents, shipment_id, reference, created_at
           FROM payments.driver_ledger_entries
          WHERE ledger_id = $1
          ORDER BY created_at ASC"
    )
    .bind(ledger_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(entry_row_to_entry).collect())
}

// ── Repository impl ────────────────────────────────────────────────────────────

#[async_trait]
impl DriverLedgerRepository for PgDriverLedgerRepository {
    async fn find_active_for_driver(
        &self,
        tenant_id: &TenantId,
        driver_id: Uuid,
    ) -> anyhow::Result<Option<DriverLedger>> {
        let row = sqlx::query_as::<_, LedgerRow>(
            "SELECT id, tenant_id, driver_id, shift_id, status,
                    balance_cents, version, created_at, updated_at
               FROM payments.driver_ledgers
              WHERE tenant_id = $1 AND driver_id = $2 AND status = 'open'
              ORDER BY created_at DESC
              LIMIT 1"
        )
        .bind(tenant_id.inner())
        .bind(driver_id)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            None => Ok(None),
            Some(r) => {
                let entries = load_entries(&self.pool, r.id).await?;
                Ok(Some(row_to_ledger(r, entries)))
            }
        }
    }

    async fn find_or_create_for_shift(
        &self,
        tenant_id: &TenantId,
        driver_id: Uuid,
        shift_id:  Option<Uuid>,
    ) -> anyhow::Result<DriverLedger> {
        // Upsert: INSERT a new ledger; on conflict (same tenant+driver+shift) return existing.
        // `shift_id` may be NULL when drivers work shiftless — unique index uses COALESCE trick.
        let row = sqlx::query_as::<_, LedgerRow>(
            r#"
            WITH ins AS (
                INSERT INTO payments.driver_ledgers
                    (id, tenant_id, driver_id, shift_id, status, balance_cents, version, created_at, updated_at)
                VALUES
                    (gen_random_uuid(), $1, $2, $3, 'open', 0, 0, NOW(), NOW())
                ON CONFLICT (tenant_id, driver_id, COALESCE(shift_id, '00000000-0000-0000-0000-000000000000'))
                DO NOTHING
                RETURNING id, tenant_id, driver_id, shift_id, status, balance_cents, version, created_at, updated_at
            )
            SELECT * FROM ins
            UNION ALL
            SELECT id, tenant_id, driver_id, shift_id, status, balance_cents, version, created_at, updated_at
              FROM payments.driver_ledgers
             WHERE tenant_id = $1
               AND driver_id = $2
               AND (shift_id = $3 OR ($3 IS NULL AND shift_id IS NULL))
               AND status = 'open'
             LIMIT 1
            "#
        )
        .bind(tenant_id.inner())
        .bind(driver_id)
        .bind(shift_id)
        .fetch_one(&self.pool)
        .await?;

        let entries = load_entries(&self.pool, row.id).await?;
        Ok(row_to_ledger(row, entries))
    }

    async fn save(&self, ledger: &DriverLedger) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;

        let new_version = ledger.version + 1;
        let updated = sqlx::query(
            "UPDATE payments.driver_ledgers
                SET status        = $1,
                    balance_cents = $2,
                    version       = $3,
                    updated_at    = NOW()
              WHERE id = $4 AND version = $5"
        )
        .bind(status_str(ledger.status))
        .bind(ledger.balance_cents)
        .bind(new_version)
        .bind(ledger.id)
        .bind(ledger.version)
        .execute(&mut *tx)
        .await?;

        if updated.rows_affected() == 0 {
            return Err(anyhow::anyhow!(
                "Optimistic lock conflict on driver_ledger {} (expected version {}). Reload and retry.",
                ledger.id, ledger.version
            ));
        }

        // Upsert all entries — ON CONFLICT DO NOTHING makes this idempotent for
        // replayed Kafka events.
        for entry in &ledger.entries {
            sqlx::query(
                "INSERT INTO payments.driver_ledger_entries
                     (id, ledger_id, kind, amount_cents, shipment_id, reference, created_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7)
                 ON CONFLICT (id) DO NOTHING"
            )
            .bind(entry.id)
            .bind(entry.ledger_id)
            .bind(kind_str(entry.kind))
            .bind(entry.amount_cents)
            .bind(entry.shipment_id)
            .bind(&entry.reference)
            .bind(entry.created_at)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }
}
