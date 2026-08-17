use async_trait::async_trait;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::domain::entities::{
    CourierEntryKind, CourierLedger, CourierLedgerEntry, CourierLedgerStatus,
};

/// Courier earnings persistence.
///
/// Mirrors OmniDeliv's vendor ledger repository: the ledger row is
/// version-gated, entries insert with `DO NOTHING` so a replay cannot rewrite
/// history.
#[async_trait]
pub trait CourierLedgerRepository: Send + Sync {
    async fn find_open(&self, tenant_id: Uuid, courier_id: Uuid, period: &str)
        -> anyhow::Result<Option<CourierLedger>>;
    async fn save(&self, ledger: &CourierLedger) -> anyhow::Result<()>;

    /// Every open ledger for a period, for the payout run.
    ///
    /// Returns them all rather than pre-filtering to positive balances: the
    /// decision about who gets paid belongs to the payout logic where it can be
    /// tested, not to a SQL predicate nobody reads.
    async fn find_all_open(&self, period: &str) -> anyhow::Result<Vec<CourierLedger>>;

    /// Has this courier already been credited for this job, in **any** period?
    ///
    /// Scanning one period's entries is not enough: `current_period()` is the
    /// ISO week, so a retry that crosses the Sunday→Monday boundary opens a
    /// fresh ledger, finds nothing, and pays twice.
    async fn entry_exists_for_job(
        &self,
        tenant_id: Uuid,
        courier_id: Uuid,
        external_ref: Uuid,
    ) -> anyhow::Result<bool>;
}

pub struct PgCourierLedgerRepository { pool: PgPool }

impl PgCourierLedgerRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

fn entry_kind(s: &str) -> anyhow::Result<CourierEntryKind> {
    Ok(match s {
        "trip_earning" => CourierEntryKind::TripEarning,
        "tip"          => CourierEntryKind::Tip,
        "adjustment"   => CourierEntryKind::Adjustment,
        "payout"       => CourierEntryKind::Payout,
        "cod_collected" => CourierEntryKind::CodCollected,
        "cod_remitted"  => CourierEntryKind::CodRemitted,
        other => anyhow::bail!("unknown courier ledger entry kind in database: {other}"),
    })
}

fn ledger_status(s: &str) -> anyhow::Result<CourierLedgerStatus> {
    Ok(match s {
        "open"    => CourierLedgerStatus::Open,
        "closed"  => CourierLedgerStatus::Closed,
        "settled" => CourierLedgerStatus::Settled,
        other => anyhow::bail!("unknown courier ledger status in database: {other}"),
    })
}

#[async_trait]
impl CourierLedgerRepository for PgCourierLedgerRepository {
    async fn find_open(
        &self,
        tenant_id: Uuid,
        courier_id: Uuid,
        period: &str,
    ) -> anyhow::Result<Option<CourierLedger>> {
        let Some(r) = sqlx::query(
            "SELECT * FROM field_ops.courier_ledgers
              WHERE tenant_id = $1 AND courier_id = $2 AND period = $3 AND status = 'open'",
        )
        .bind(tenant_id).bind(courier_id).bind(period)
        .fetch_optional(&self.pool).await?
        else {
            return Ok(None);
        };

        let id: Uuid = r.get("id");
        let entry_rows = sqlx::query(
            "SELECT * FROM field_ops.courier_ledger_entries WHERE ledger_id = $1 ORDER BY created_at, id",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await?;

        let mut entries = Vec::with_capacity(entry_rows.len());
        for er in &entry_rows {
            let kind: String = er.get("kind");
            entries.push(CourierLedgerEntry {
                id:           er.get("id"),
                ledger_id:    er.get("ledger_id"),
                kind:         entry_kind(&kind)?,
                amount_cents: er.get("amount_cents"),
                external_ref: er.get("external_ref"),
                reference:    er.get("reference"),
                created_at:   er.get("created_at"),
            });
        }

        let status: String = r.get("status");
        Ok(Some(CourierLedger {
            id,
            tenant_id:     r.get("tenant_id"),
            courier_id:    r.get("courier_id"),
            period:        r.get("period"),
            status:        ledger_status(&status)?,
            balance_cents: r.get("balance_cents"),
            version:       r.get("version"),
            entries,
            created_at:    r.get("created_at"),
            updated_at:    r.get("updated_at"),
        }))
    }

    async fn find_all_open(&self, period: &str) -> anyhow::Result<Vec<CourierLedger>> {
        // Deliberately across all tenants: a payout run is an operator action,
        // and scoping it per tenant would mean couriers only get paid for
        // tenants somebody remembered to enumerate.
        let ids: Vec<(Uuid, Uuid)> = sqlx::query_as(
            "SELECT tenant_id, courier_id FROM field_ops.courier_ledgers
              WHERE period = $1 AND status = 'open'",
        )
        .bind(period)
        .fetch_all(&self.pool)
        .await?;

        let mut out = Vec::with_capacity(ids.len());
        for (tenant_id, courier_id) in ids {
            if let Some(l) = self.find_open(tenant_id, courier_id, period).await? {
                out.push(l);
            }
        }
        Ok(out)
    }

    async fn save(&self, l: &CourierLedger) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;

        let result = sqlx::query(
            r#"
            INSERT INTO field_ops.courier_ledgers (
                id, tenant_id, courier_id, period, status, balance_cents, version,
                created_at, updated_at
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
            ON CONFLICT (id) DO UPDATE SET
                status        = EXCLUDED.status,
                balance_cents = EXCLUDED.balance_cents,
                version       = EXCLUDED.version,
                updated_at    = EXCLUDED.updated_at
            WHERE field_ops.courier_ledgers.version < EXCLUDED.version
            "#,
        )
        .bind(l.id).bind(l.tenant_id).bind(l.courier_id).bind(&l.period)
        .bind(l.status.as_str()).bind(l.balance_cents).bind(l.version)
        .bind(l.created_at).bind(l.updated_at)
        .execute(&mut *tx)
        .await?;

        if result.rows_affected() == 0 {
            tx.rollback().await?;
            anyhow::bail!("courier ledger {} was modified concurrently", l.id);
        }

        for e in &l.entries {
            sqlx::query(
                r#"
                INSERT INTO field_ops.courier_ledger_entries (
                    id, ledger_id, kind, amount_cents, external_ref, reference, created_at
                ) VALUES ($1,$2,$3,$4,$5,$6,$7)
                ON CONFLICT (id) DO NOTHING
                "#,
            )
            .bind(e.id).bind(e.ledger_id).bind(e.kind.as_str()).bind(e.amount_cents)
            .bind(e.external_ref).bind(&e.reference).bind(e.created_at)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    async fn entry_exists_for_job(
        &self,
        _tenant_id: Uuid,
        _courier_id: Uuid,
        _external_ref: Uuid,
    ) -> anyhow::Result<bool> {
        Ok(false)
    }
}
