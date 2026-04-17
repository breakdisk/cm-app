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
    sqlx::query(&ddl).execute(pool).await?;

    migrator.run(pool).await?;
    Ok(())
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
