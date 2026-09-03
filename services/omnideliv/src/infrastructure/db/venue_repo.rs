//! Venues, tables, and the sessions a scan opens.
//!
//! `find_table_by_token` is the only query in this service that resolves across
//! tenants — see the exception note in `domain::repositories`. Everything else
//! here is tenant-scoped in the usual way.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::domain::entities::{
    OpeningWindow, Table, TableSession, TableStatus, Venue, VenueKind, VenueStatus,
};
use crate::domain::repositories::VenueRepository;

pub struct PgVenueRepository {
    pool: PgPool,
}

impl PgVenueRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// Opening hours are stored as JSONB. A row whose schedule cannot be parsed is
/// treated as having NO hours, which `Venue::is_open_at` reads as closed.
///
/// Deliberately fail-closed: a malformed schedule is a configuration error, and
/// the safe reading of "I cannot tell when this venue is open" is "not now",
/// not "always".
fn parse_hours(raw: serde_json::Value) -> Vec<OpeningWindow> {
    serde_json::from_value(raw).unwrap_or_default()
}

fn venue_from_row(r: &sqlx::postgres::PgRow) -> anyhow::Result<Venue> {
    let kind: String = r.get("kind");
    let status: String = r.get("status");
    Ok(Venue {
        id: r.get("id"),
        tenant_id: r.get("tenant_id"),
        name: r.get("name"),
        kind: VenueKind::from_wire(&kind)
            .ok_or_else(|| anyhow::anyhow!("unknown venue kind: {kind}"))?,
        hours: parse_hours(r.get("hours")),
        utc_offset_minutes: r.get("utc_offset_minutes"),
        status: VenueStatus::from_wire(&status)
            .ok_or_else(|| anyhow::anyhow!("unknown venue status: {status}"))?,
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    })
}

fn table_from_row(r: &sqlx::postgres::PgRow) -> anyhow::Result<Table> {
    let status: String = r.get("t_status");
    Ok(Table {
        id: r.get("t_id"),
        venue_id: r.get("t_venue_id"),
        tenant_id: r.get("t_tenant_id"),
        label: r.get("t_label"),
        token: r.get("t_token"),
        status: TableStatus::from_wire(&status)
            .ok_or_else(|| anyhow::anyhow!("unknown table status: {status}"))?,
        printed_at: r.get("t_printed_at"),
        created_at: r.get("t_created_at"),
        updated_at: r.get("t_updated_at"),
    })
}

#[async_trait]
impl VenueRepository for PgVenueRepository {
    async fn find_table_by_token(&self, token: &str) -> anyhow::Result<Option<(Table, Venue)>> {
        // One round trip, not two. A scan is the latency a diner actually feels,
        // standing at a table with a phone in their hand.
        //
        // The table's columns are aliased because `venues` and `tables` share
        // several names (id, tenant_id, status, created_at) and an unqualified
        // read would silently take whichever the driver saw last.
        let row = sqlx::query(
            r#"
            SELECT v.id, v.tenant_id, v.name, v.kind, v.hours, v.utc_offset_minutes,
                   v.status, v.created_at, v.updated_at,
                   t.id         AS t_id,
                   t.venue_id   AS t_venue_id,
                   t.tenant_id  AS t_tenant_id,
                   t.label      AS t_label,
                   t.token      AS t_token,
                   t.status     AS t_status,
                   t.printed_at AS t_printed_at,
                   t.created_at AS t_created_at,
                   t.updated_at AS t_updated_at
              FROM omnideliv.tables t
              JOIN omnideliv.venues v ON v.id = t.venue_id
             WHERE t.token = $1
            "#,
        )
        .bind(token)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(r) => Ok(Some((table_from_row(&r)?, venue_from_row(&r)?))),
            None => Ok(None),
        }
    }

    async fn count_live_sessions(&self, table_id: Uuid, now: DateTime<Utc>) -> anyhow::Result<i64> {
        // Live means unended AND unexpired, matching `TableSession::is_live`.
        // Expiry is part of the predicate so an abandoned party ages out of the
        // cap on its own rather than holding a seat until someone notices.
        let row = sqlx::query(
            r#"
            SELECT COUNT(*) AS n
              FROM omnideliv.table_sessions
             WHERE table_id = $1 AND ended_at IS NULL AND expires_at > $2
            "#,
        )
        .bind(table_id)
        .bind(now)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.get("n"))
    }

    async fn create_session(&self, s: &TableSession) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO omnideliv.table_sessions
                (id, table_id, venue_id, tenant_id, created_at, expires_at, ended_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
        )
        .bind(s.id)
        .bind(s.table_id)
        .bind(s.venue_id)
        .bind(s.tenant_id)
        .bind(s.created_at)
        .bind(s.expires_at)
        .bind(s.ended_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_tables(&self, tenant_id: Uuid, venue_id: Uuid) -> anyhow::Result<Vec<Table>> {
        let rows = sqlx::query(
            r#"
            SELECT id AS t_id, venue_id AS t_venue_id, tenant_id AS t_tenant_id,
                   label AS t_label, token AS t_token, status AS t_status,
                   printed_at AS t_printed_at, created_at AS t_created_at,
                   updated_at AS t_updated_at
              FROM omnideliv.tables
             WHERE tenant_id = $1 AND venue_id = $2
             ORDER BY label ASC
            "#,
        )
        .bind(tenant_id)
        .bind(venue_id)
        .fetch_all(&self.pool)
        .await?;

        rows.iter().map(table_from_row).collect()
    }

    async fn find_live_session(
        &self,
        tenant_id: Uuid,
        session_id: Uuid,
        now: DateTime<Utc>,
    ) -> anyhow::Result<Option<TableSession>> {
        // Liveness is in the predicate, not checked afterwards: an expired row
        // must read as absent, so a token that outlived its session cannot
        // still order.
        let row = sqlx::query(
            r#"
            SELECT id, table_id, venue_id, tenant_id, created_at, expires_at, ended_at
              FROM omnideliv.table_sessions
             WHERE id = $1 AND tenant_id = $2 AND ended_at IS NULL AND expires_at > $3
            "#,
        )
        .bind(session_id)
        .bind(tenant_id)
        .bind(now)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| TableSession {
            id: r.get("id"),
            table_id: r.get("table_id"),
            venue_id: r.get("venue_id"),
            tenant_id: r.get("tenant_id"),
            created_at: r.get("created_at"),
            expires_at: r.get("expires_at"),
            ended_at: r.get("ended_at"),
        }))
    }

    async fn vendor_is_at_venue(
        &self,
        tenant_id: Uuid,
        venue_id: Uuid,
        vendor_id: Uuid,
    ) -> anyhow::Result<bool> {
        let row = sqlx::query(
            r#"
            SELECT 1 AS ok FROM omnideliv.venue_vendors
             WHERE tenant_id = $1 AND venue_id = $2 AND vendor_id = $3
            "#,
        )
        .bind(tenant_id)
        .bind(venue_id)
        .bind(vendor_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.is_some())
    }

    async fn rotate_token(
        &self,
        tenant_id: Uuid,
        table_id: Uuid,
        new_token: &str,
    ) -> anyhow::Result<bool> {
        // Scoped by tenant so a caller cannot rotate another tenant's code by
        // guessing a table id. `printed_at` is cleared, not stamped: the new
        // code is by definition not on paper yet, and leaving the old timestamp
        // would tell an operator a table is printed when it is not.
        let n = sqlx::query(
            r#"
            UPDATE omnideliv.tables
               SET token = $3, printed_at = NULL, updated_at = NOW()
             WHERE id = $1 AND tenant_id = $2
            "#,
        )
        .bind(table_id)
        .bind(tenant_id)
        .bind(new_token)
        .execute(&self.pool)
        .await?
        .rows_affected();
        Ok(n == 1)
    }

    async fn create_venue(&self, v: &Venue) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO omnideliv.venues
                (id, tenant_id, name, kind, hours, utc_offset_minutes, status,
                 created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
        )
        .bind(v.id)
        .bind(v.tenant_id)
        .bind(&v.name)
        .bind(v.kind.as_str())
        .bind(serde_json::to_value(&v.hours)?)
        .bind(v.utc_offset_minutes)
        .bind(v.status.as_str())
        .bind(v.created_at)
        .bind(v.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_venues(&self, tenant_id: Uuid) -> anyhow::Result<Vec<Venue>> {
        let rows = sqlx::query(
            r#"
            SELECT id, tenant_id, name, kind, hours, utc_offset_minutes, status,
                   created_at, updated_at
              FROM omnideliv.venues
             WHERE tenant_id = $1
             ORDER BY created_at DESC
            "#,
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await?;

        rows.iter().map(venue_from_row).collect()
    }

    async fn find_venue(&self, tenant_id: Uuid, venue_id: Uuid) -> anyhow::Result<Option<Venue>> {
        let row = sqlx::query(
            r#"
            SELECT id, tenant_id, name, kind, hours, utc_offset_minutes, status,
                   created_at, updated_at
              FROM omnideliv.venues
             WHERE tenant_id = $1 AND id = $2
            "#,
        )
        .bind(tenant_id)
        .bind(venue_id)
        .fetch_optional(&self.pool)
        .await?;

        row.as_ref().map(venue_from_row).transpose()
    }

    async fn create_tables(&self, tables: &[Table]) -> anyhow::Result<()> {
        // One transaction: `UNIQUE (venue_id, label)` means a clash halfway
        // through a batch must not leave the earlier tables created. The
        // operator retries the whole list, not the part that failed.
        let mut tx = self.pool.begin().await?;
        for t in tables {
            sqlx::query(
                r#"
                INSERT INTO omnideliv.tables
                    (id, venue_id, tenant_id, label, token, status, printed_at,
                     created_at, updated_at)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                "#,
            )
            .bind(t.id)
            .bind(t.venue_id)
            .bind(t.tenant_id)
            .bind(&t.label)
            .bind(&t.token)
            .bind(t.status.as_str())
            .bind(t.printed_at)
            .bind(t.created_at)
            .bind(t.updated_at)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn link_vendor(
        &self,
        tenant_id: Uuid,
        venue_id:  Uuid,
        vendor_id: Uuid,
    ) -> anyhow::Result<bool> {
        // Both sides are checked against the tenant BEFORE the insert. The
        // foreign keys only prove the rows exist, not whose they are, so
        // without this a caller could hang their own vendor off another
        // tenant's venue and become orderable at that venue's tables.
        let ok = sqlx::query(
            r#"
            SELECT 1 AS ok
             WHERE EXISTS (SELECT 1 FROM omnideliv.venues  WHERE id = $2 AND tenant_id = $1)
               AND EXISTS (SELECT 1 FROM omnideliv.vendors WHERE id = $3 AND tenant_id = $1)
            "#,
        )
        .bind(tenant_id)
        .bind(venue_id)
        .bind(vendor_id)
        .fetch_optional(&self.pool)
        .await?
        .is_some();

        if !ok {
            return Ok(false);
        }

        // Idempotent: an operator clicking twice is not an error, and the
        // second click must still report success or the UI shows a failure for
        // a state that is exactly what was asked for.
        sqlx::query(
            r#"
            INSERT INTO omnideliv.venue_vendors (venue_id, vendor_id, tenant_id)
            VALUES ($1, $2, $3)
            ON CONFLICT (venue_id, vendor_id) DO NOTHING
            "#,
        )
        .bind(venue_id)
        .bind(vendor_id)
        .bind(tenant_id)
        .execute(&self.pool)
        .await?;

        Ok(true)
    }

    async fn unlink_vendor(
        &self,
        tenant_id: Uuid,
        venue_id:  Uuid,
        vendor_id: Uuid,
    ) -> anyhow::Result<bool> {
        let n = sqlx::query(
            r#"
            DELETE FROM omnideliv.venue_vendors
             WHERE tenant_id = $1 AND venue_id = $2 AND vendor_id = $3
            "#,
        )
        .bind(tenant_id)
        .bind(venue_id)
        .bind(vendor_id)
        .execute(&self.pool)
        .await?
        .rows_affected();
        Ok(n == 1)
    }

    async fn list_venue_vendors(
        &self,
        tenant_id: Uuid,
        venue_id:  Uuid,
    ) -> anyhow::Result<Vec<(Uuid, String)>> {
        let rows = sqlx::query(
            r#"
            SELECT v.id AS vendor_id, v.name AS vendor_name
              FROM omnideliv.venue_vendors vv
              JOIN omnideliv.vendors v ON v.id = vv.vendor_id
             WHERE vv.tenant_id = $1 AND vv.venue_id = $2
             ORDER BY v.name ASC
            "#,
        )
        .bind(tenant_id)
        .bind(venue_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .iter()
            .map(|r| (r.get("vendor_id"), r.get("vendor_name")))
            .collect())
    }

    async fn mark_printed(
        &self,
        tenant_id: Uuid,
        table_id:  Uuid,
        now:       DateTime<Utc>,
    ) -> anyhow::Result<bool> {
        let n = sqlx::query(
            r#"
            UPDATE omnideliv.tables
               SET printed_at = $3, updated_at = NOW()
             WHERE id = $1 AND tenant_id = $2
            "#,
        )
        .bind(table_id)
        .bind(tenant_id)
        .bind(now)
        .execute(&self.pool)
        .await?
        .rows_affected();
        Ok(n == 1)
    }
}
