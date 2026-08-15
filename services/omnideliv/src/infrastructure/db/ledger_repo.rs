use async_trait::async_trait;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::domain::entities::{EntryKind, LedgerEntry, LedgerStatus, TelemetryEvent, VendorLedger};
use crate::domain::repositories::{LedgerPeriod, TelemetryRepository, VendorLedgerRepository};

pub struct PgVendorLedgerRepository { pool: PgPool }

impl PgVendorLedgerRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

fn entry_kind(s: &str) -> anyhow::Result<EntryKind> {
    Ok(match s {
        "goods_credit"     => EntryKind::GoodsCredit,
        "commission_debit" => EntryKind::CommissionDebit,
        "adjustment"       => EntryKind::Adjustment,
        "payout"           => EntryKind::Payout,
        other => anyhow::bail!("unknown ledger entry kind in database: {other}"),
    })
}

fn ledger_status(s: &str) -> anyhow::Result<LedgerStatus> {
    Ok(match s {
        "open"    => LedgerStatus::Open,
        "closed"  => LedgerStatus::Closed,
        "settled" => LedgerStatus::Settled,
        other => anyhow::bail!("unknown ledger status in database: {other}"),
    })
}

#[async_trait]
impl VendorLedgerRepository for PgVendorLedgerRepository {
    async fn list_recent(
        &self,
        tenant_id: Uuid,
        vendor_id: Uuid,
        limit: i64,
    ) -> anyhow::Result<Vec<LedgerPeriod>> {
        // Period is a sortable `YYYY-Www`/`YYYY-MM` string (see `current_period`),
        // so ordering on it lexically is ordering it chronologically. No entries
        // are joined — this feeds three totals on a card.
        let rows = sqlx::query(
            "SELECT period, status, balance_cents, updated_at
               FROM omnideliv.vendor_ledgers
              WHERE tenant_id = $1 AND vendor_id = $2
              ORDER BY period DESC
              LIMIT $3",
        )
        .bind(tenant_id).bind(vendor_id).bind(limit)
        .fetch_all(&self.pool)
        .await?;

        let mut out = Vec::with_capacity(rows.len());
        for r in &rows {
            let status: String = r.get("status");
            out.push(LedgerPeriod {
                period:        r.get("period"),
                status:        ledger_status(&status)?,
                balance_cents: r.get("balance_cents"),
                updated_at:    r.get("updated_at"),
            });
        }
        Ok(out)
    }

    async fn find_open(
        &self,
        tenant_id: Uuid,
        vendor_id: Uuid,
        period: &str,
    ) -> anyhow::Result<Option<VendorLedger>> {
        let Some(r) = sqlx::query(
            "SELECT * FROM omnideliv.vendor_ledgers
              WHERE tenant_id = $1 AND vendor_id = $2 AND period = $3 AND status = 'open'",
        )
        .bind(tenant_id).bind(vendor_id).bind(period)
        .fetch_optional(&self.pool).await?
        else {
            return Ok(None);
        };

        let id: Uuid = r.get("id");
        let entry_rows = sqlx::query(
            "SELECT * FROM omnideliv.vendor_ledger_entries WHERE ledger_id = $1 ORDER BY created_at, id",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await?;

        let mut entries = Vec::with_capacity(entry_rows.len());
        for er in &entry_rows {
            let kind: String = er.get("kind");
            entries.push(LedgerEntry {
                id:           er.get("id"),
                ledger_id:    er.get("ledger_id"),
                kind:         entry_kind(&kind)?,
                amount_cents: er.get("amount_cents"),
                order_id:     er.get("order_id"),
                leg_id:       er.get("leg_id"),
                reference:    er.get("reference"),
                created_at:   er.get("created_at"),
            });
        }

        let status: String = r.get("status");
        Ok(Some(VendorLedger {
            id,
            tenant_id:     r.get("tenant_id"),
            vendor_id:     r.get("vendor_id"),
            period:        r.get("period"),
            status:        ledger_status(&status)?,
            balance_cents: r.get("balance_cents"),
            version:       r.get("version"),
            entries,
            created_at:    r.get("created_at"),
            updated_at:    r.get("updated_at"),
        }))
    }

    async fn save(&self, l: &VendorLedger) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;

        // Version-gated like the basket: two concurrent collections crediting
        // the same vendor must not lose an entry to a last-write-wins race.
        let result = sqlx::query(
            r#"
            INSERT INTO omnideliv.vendor_ledgers (
                id, tenant_id, vendor_id, period, status, balance_cents, version,
                created_at, updated_at
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
            ON CONFLICT (id) DO UPDATE SET
                status        = EXCLUDED.status,
                balance_cents = EXCLUDED.balance_cents,
                version       = EXCLUDED.version,
                updated_at    = EXCLUDED.updated_at
            WHERE omnideliv.vendor_ledgers.version < EXCLUDED.version
            "#,
        )
        .bind(l.id).bind(l.tenant_id).bind(l.vendor_id).bind(&l.period)
        .bind(l.status.as_str()).bind(l.balance_cents).bind(l.version)
        .bind(l.created_at).bind(l.updated_at)
        .execute(&mut *tx)
        .await?;

        if result.rows_affected() == 0 {
            tx.rollback().await?;
            anyhow::bail!("vendor ledger {} was modified concurrently", l.id);
        }

        // Entries are inserted, never updated: ON CONFLICT DO NOTHING rather
        // than DO UPDATE, so a replayed save cannot rewrite history. That is the
        // append-only guarantee the REVOKE cannot enforce while services run as
        // the schema owner.
        for e in &l.entries {
            sqlx::query(
                r#"
                INSERT INTO omnideliv.vendor_ledger_entries (
                    id, ledger_id, kind, amount_cents, order_id, leg_id, reference, created_at
                ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
                ON CONFLICT (id) DO NOTHING
                "#,
            )
            .bind(e.id).bind(e.ledger_id).bind(e.kind.as_str()).bind(e.amount_cents)
            .bind(e.order_id).bind(e.leg_id).bind(&e.reference).bind(e.created_at)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }
}

pub struct PgTelemetryRepository { pool: PgPool }

impl PgTelemetryRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[async_trait]
impl TelemetryRepository for PgTelemetryRepository {
    async fn append(&self, e: &TelemetryEvent) -> anyhow::Result<()> {
        // Plain INSERT with no ON CONFLICT: this table is append-only, and a
        // conflict clause would be a place for a future edit to hide.
        sqlx::query(
            r#"
            INSERT INTO omnideliv.order_telemetry_logs (
                id, order_id, tenant_id, event_type, device_timestamp,
                server_timestamp, actor_id, payload
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
            "#,
        )
        .bind(e.id).bind(e.order_id).bind(e.tenant_id).bind(&e.event_type)
        .bind(e.device_timestamp).bind(e.server_timestamp).bind(e.actor_id).bind(&e.payload)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn timeline(&self, tenant_id: Uuid, order_id: Uuid) -> anyhow::Result<Vec<TelemetryEvent>> {
        let rows = sqlx::query(
            "SELECT * FROM omnideliv.order_telemetry_logs
              WHERE tenant_id = $1 AND order_id = $2
              ORDER BY server_timestamp ASC",
        )
        .bind(tenant_id).bind(order_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .iter()
            .map(|r| TelemetryEvent {
                id:               r.get("id"),
                order_id:         r.get("order_id"),
                tenant_id:        r.get("tenant_id"),
                event_type:       r.get("event_type"),
                device_timestamp: r.get("device_timestamp"),
                server_timestamp: r.get("server_timestamp"),
                actor_id:         r.get("actor_id"),
                payload:          r.get("payload"),
            })
            .collect())
    }
}
