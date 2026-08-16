//! Schema-isolated migration runner for the schema-per-service Postgres layout.
//!
//! sqlx 0.7's `sqlx::migrate!()` tracks state in a `_sqlx_migrations` table created
//! via `CREATE TABLE IF NOT EXISTS _sqlx_migrations` — an **unqualified** name. With
//! our `search_path = <service_schema>, public`, if `public._sqlx_migrations` was ever
//! created (e.g. by a service that ran before its `after_connect` hook was in place),
//! PostgreSQL's name resolution finds it there first and sqlx silently reads/writes
//! migration state against `public`. Every service then shares one tracking table and
//! cross-contaminates version numbers. Symptom: `public._sqlx_migrations` marks a
//! service's migrations `success=true` while that service's schema is empty — so
//! subsequent restarts skip migration and the service cannot find its own tables.
//!
//! This helper eliminates the footgun by pre-creating `<schema>._sqlx_migrations` with
//! sqlx's exact DDL. Because `<schema>` sits first in `search_path`, every unqualified
//! reference sqlx emits afterward resolves to the service-owned table — not `public`.

use sqlx::PgPool;
use sqlx::migrate::Migrator;

/// Run migrations against a service-owned `_sqlx_migrations` table in `schema`.
///
/// Call this instead of `migrator.run(&pool)` directly. The `schema` must match the
/// first entry of the connection `search_path`.
pub async fn run(
    pool: &PgPool,
    schema: &str,
    migrator: &Migrator,
) -> Result<(), sqlx::Error> {
    validate_schema_ident(schema)?;

    // Everything below runs on one connection, under one advisory lock.
    //
    // `IF NOT EXISTS` is a check, not an atomic operation. Two callers can both
    // find the table absent and both try to create it, and the loser gets
    // `duplicate key value violates unique constraint "pg_type_typname_nsp_index"`
    // — pg_type, not pg_class, because every CREATE TABLE also creates a
    // composite type of the same name.
    //
    // sqlx's own `Migrator::run` already takes an advisory lock, so the
    // migrations themselves were safe. This prelude ran *before* it, unlocked.
    //
    // Not a theoretical race. It fails the field-ops claim_race suite whenever
    // two test binaries start together, and the platform requires zero-downtime
    // rolling updates — which means two replicas booting at once is the normal
    // case, not the exception.
    let mut conn = pool.acquire().await?;

    // Keyed on the schema so services do not serialise against each other, and
    // derived rather than assigned so adding a service needs no new constant.
    let key = advisory_key(schema);
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(key)
        .execute(&mut *conn)
        .await?;

    let guarded = async {
        // Schema must exist before we can create the tracking table inside it.
        // Migration 0001 typically creates the schema, but it can't run until
        // the migrator has somewhere to record its state — chicken/egg.
        let create_schema = format!(r#"CREATE SCHEMA IF NOT EXISTS "{schema}""#);
        sqlx::query(&create_schema).execute(&mut *conn).await?;

        let ddl = format!(
            r#"CREATE TABLE IF NOT EXISTS "{schema}"._sqlx_migrations (
                version BIGINT PRIMARY KEY,
                description TEXT NOT NULL,
                installed_on TIMESTAMPTZ NOT NULL DEFAULT now(),
                success BOOLEAN NOT NULL,
                checksum BYTEA NOT NULL,
                execution_time BIGINT NOT NULL
            )"#
        );
        sqlx::query(&ddl).execute(&mut *conn).await?;
        Ok::<_, sqlx::Error>(())
    }
    .await;

    // Released explicitly rather than left to connection teardown: the
    // connection returns to the pool and would carry the lock with it, so the
    // next caller to be handed that connection would deadlock against itself.
    // Unlocked even when the DDL failed, or one bad start wedges every later one.
    let unlock = sqlx::query("SELECT pg_advisory_unlock($1)")
        .bind(key)
        .execute(&mut *conn)
        .await;

    drop(conn);
    guarded?;
    unlock?;

    migrator.run(pool).await?;
    Ok(())
}

/// A stable 64-bit advisory-lock key for a schema name.
///
/// FNV-1a rather than `DefaultHasher`: `std`'s hasher is explicitly not
/// guaranteed stable across releases, and two binaries built against different
/// toolchains would then take *different* locks for the same schema and race
/// anyway — the exact failure this is meant to prevent, made invisible.
fn advisory_key(schema: &str) -> i64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in schema.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    hash as i64
}

fn validate_schema_ident(schema: &str) -> Result<(), sqlx::Error> {
    let ok = !schema.is_empty()
        && schema.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !schema.starts_with(|c: char| c.is_ascii_digit());
    if ok {
        Ok(())
    } else {
        Err(sqlx::Error::Configuration(
            format!("invalid schema identifier: {schema:?}").into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::advisory_key;

    /// Pins the constant. The key is a wire protocol between processes — two
    /// binaries built at different times must derive the *same* number for the
    /// same schema, or they take different locks and race while appearing to be
    /// protected. Changing this value is a breaking change, not a refactor.
    #[test]
    fn the_key_for_a_schema_is_a_fixed_number() {
        assert_eq!(advisory_key("field_ops"), 1_879_162_965_117_966_260);
    }

    /// Per-schema, so migrating one service does not block every other service
    /// starting at the same time — which is exactly what a rolling deploy does.
    #[test]
    fn different_schemas_take_different_locks() {
        assert_ne!(advisory_key("field_ops"), advisory_key("omnideliv"));
        assert_ne!(advisory_key("marketing"), advisory_key("identity"));
    }

    #[test]
    fn the_same_schema_always_takes_the_same_lock() {
        assert_eq!(advisory_key("payments"), advisory_key("payments"));
    }
}
