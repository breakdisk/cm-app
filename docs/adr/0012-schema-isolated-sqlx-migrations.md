# ADR-0012: Schema-Isolated sqlx Migration Tracking

Date: 2026-04-17
Status: Accepted
Deciders: Principal Software Architect

## Context

LogisticOS runs 17 Rust microservices against a single PostgreSQL instance with
schema-per-service isolation (`order_intake`, `dispatch`, `pod`, etc.). Per
ADR-0005, each service owns its schema exclusively. Each service also owns its
own migration set under `services/<svc>/migrations/` and applies them at startup
with `sqlx::migrate!("./migrations").run(&pool)`.

sqlx 0.7 tracks applied migrations in a `_sqlx_migrations` table created via the
following unqualified DDL:

```sql
CREATE TABLE IF NOT EXISTS _sqlx_migrations (...);
```

The services configure `after_connect` to set:

```sql
SET search_path TO <service_schema>, public;
```

### The failure mode

If `public._sqlx_migrations` exists (created by any historical deploy where the
`after_connect` hook had not yet been added, or by manual operator work),
PostgreSQL's name-resolution semantics make sqlx's unqualified statements
resolve against **`public._sqlx_migrations`** — not the service's schema:

1. `CREATE TABLE IF NOT EXISTS _sqlx_migrations` — `IF NOT EXISTS` is satisfied
   by the existing `public._sqlx_migrations` visible via the `public` element of
   `search_path`. sqlx emits a `NOTICE: relation "_sqlx_migrations" already
   exists, skipping` and proceeds.
2. `SELECT version FROM _sqlx_migrations` — resolves to `public._sqlx_migrations`.
3. `INSERT INTO _sqlx_migrations ...` — writes to `public._sqlx_migrations`.

All services end up sharing **one** tracking table keyed only by `version`.
Cross-service version collisions silently mark migrations from service B as
"already applied" when only service A's migrations actually ran. The observed
production symptom: `order_intake` schema exists but is empty, while
`public._sqlx_migrations` holds `success=true` rows for order-intake's
versions 1-7, so the service skips migration on every restart and all
shipment queries fail with `relation "order_intake.shipments" does not exist`.

sqlx 0.7's `Migrator` API does not expose a way to override the tracking-table
name or schema.

## Decision

Pre-create `<service_schema>._sqlx_migrations` with the exact sqlx DDL *before*
calling `migrator.run()`. Because `<service_schema>` is the first element of
`search_path`, every unqualified reference sqlx emits afterward resolves to the
service-owned table — `public._sqlx_migrations` becomes invisible to sqlx even
if it exists.

This is centralized in `libs/common/src/migrations.rs` as
`logisticos_common::migrations::run(pool, schema, migrator)`. All 17 services
call this helper instead of `migrator.run(pool)` directly.

## Consequences

### Positive

- Each service owns its migration tracking state in its own schema, consistent
  with ADR-0005 schema-per-service isolation.
- `public._sqlx_migrations` — if it exists at all — is inert and cannot poison
  migration runs.
- Cross-service version collisions are impossible: each service has its own
  `version` primary key space.
- Schema-level backup/restore covers migration state along with application
  data; restoring a single schema does not desync migrations.
- When DDL drift is introduced by future sqlx versions, the helper is the
  single point of update across the fleet.

### Negative

- The helper embeds a verbatim copy of sqlx 0.7's `_sqlx_migrations` DDL. Any
  sqlx schema change in a future release requires updating the helper in lockstep.
  Mitigation: the DDL is well-known and stable across sqlx 0.5→0.7; we pin sqlx
  at the workspace level and test on upgrade.
- The schema identifier is interpolated into a DDL string. The helper validates
  the identifier as `[A-Za-z_][A-Za-z0-9_]*` before use to prevent injection;
  the identifier is always a compile-time constant at the call site.

### Neutral

- Existing deployments with poisoned `public._sqlx_migrations` rows need a
  one-time cleanup. See `scripts/migrations/0012_cleanup_public_migrations.sql`.

## Implementation

`libs/common/src/migrations.rs`:

```rust
pub async fn run(pool: &PgPool, schema: &str, migrator: &Migrator)
    -> Result<(), sqlx::Error>
{
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
```

Each service's `bootstrap.rs`:

```rust
// Before
sqlx::migrate!("./migrations").run(&pool).await?;

// After
logisticos_common::migrations::run(&pool, "order_intake", &sqlx::migrate!("./migrations")).await?;
```

## Production Cleanup

For existing deployments, run
`scripts/migrations/0012_cleanup_public_migrations.sql` once against the shared
database. It audits `public._sqlx_migrations`, moves any orphaned rows into the
correct per-service schema's tracking table, and then drops `public._sqlx_migrations`.

After cleanup, restart each service. The service-owned `_sqlx_migrations`
tables already exist (they were created by the new helper at startup); the
migrator re-reads them, sees the migration state moved by the cleanup script,
and applies only the missing migrations.

## References

- ADR-0005: Hexagonal architecture for microservices
- ADR-0008: Multi-tenancy RLS strategy
- sqlx 0.7 Migrator: https://docs.rs/sqlx/0.7/sqlx/migrate/struct.Migrator.html
- PostgreSQL search_path docs:
  https://www.postgresql.org/docs/current/ddl-schemas.html#DDL-SCHEMAS-PATH
