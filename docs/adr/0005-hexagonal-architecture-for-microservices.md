# ADR-0005: Hexagonal Architecture (Ports & Adapters) for All Rust Microservices

**Status:** Accepted
**Date:** 2026-03-17
**Deciders:** Principal Architect, Staff Engineer — Rust Platform, Engineering Manager — Platform Core, CTO

---

## Context

As LogisticOS scales from one service to 17, each Rust microservice faces a structural decision: how should domain logic, infrastructure, and API concerns be organized? Without an enforced architectural pattern, individual services will evolve toward different conventions, creating onboarding friction and making cross-service reasoning difficult.

The specific pain points driving this decision:

1. **Brittle integration tests** — tests that target domain logic today spin up actual PostgreSQL containers because domain code imports SQLx directly. A developer changing a repository query must set up a full DB fixture just to test a business rule.

2. **Infrastructure lock-in** — `dispatch` service uses PostGIS-specific SQL extensively in service structs. Swapping the geospatial engine (or adding a secondary read store) requires invasive refactoring across business logic.

3. **Unclear dependency direction** — engineers in different services make inconsistent choices about where to put validation, error mapping, and event publishing. A senior engineer joining a new service wastes hours tracing where logic lives.

4. **Difficult MCP layer integration** — MCP tool handlers (per ADR-0004) need to call application-level use cases. Without a defined application layer, MCP handlers call service structs directly, duplicating HTTP handler logic.

Alternatives evaluated in the context of this decision:

- **Layered (N-tier) architecture** — domain → service → repository → infrastructure. Common in Java Spring. Rejected because it does not enforce the critical dependency rule: in a naive layered setup, service code still imports infrastructure types (e.g., `PgPool`), which prevents in-memory test doubles.
- **CQRS + Event Sourcing** — deferred; considered for the dispatch and payments services in a later ADR where the command/query separation provides measurable value. Too heavy for identity, hub-ops, and carrier services at this stage.
- **Actor model (Actix)** — explored for driver-ops due to the per-driver state machine. Rejected for broad adoption due to additional complexity; actor model may be used within a service internally while still conforming to the hexagonal boundary.

---

## Decision

All 17 LogisticOS Rust microservices adopt **Hexagonal Architecture (Ports & Adapters)**, organized into four distinct layers with a strict inward dependency rule.

### The Four Layers

```
┌─────────────────────────────────────────────────────────────────────┐
│                          API LAYER                                  │
│  HTTP handlers (Axum), gRPC handlers (Tonic), MCP tool handlers     │
│  WebSocket handlers, CLI (for operational scripts)                  │
│  → Translates protocol-level concerns to/from application commands  │
└──────────────────────────────┬──────────────────────────────────────┘
                               │ calls
┌──────────────────────────────▼──────────────────────────────────────┐
│                       APPLICATION LAYER                             │
│  Use case services (one struct per use case group)                  │
│  Command types, Query types, Response DTOs                          │
│  Orchestrates domain objects; owns transaction boundaries           │
│  → Depends only on domain traits (ports); no infra imports          │
└──────────────────────────────┬──────────────────────────────────────┘
                               │ calls traits from
┌──────────────────────────────▼──────────────────────────────────────┐
│                        DOMAIN LAYER                                 │
│  Entities (e.g., Shipment, Driver, Route)                           │
│  Value objects (e.g., TrackingNumber, GeoPoint, Money)              │
│  Repository traits (ports): ShipmentRepository, DriverRepository   │
│  Domain events: ShipmentCreatedEvent, DriverAssignedEvent           │
│  Domain errors: DomainError enum with thiserror                     │
│  → Zero external imports. Pure Rust, no async runtime dependency.   │
└─────────────────────────────────────────────────────────────────────┘
         ▲ implements traits from            ▲ implements traits from
┌────────┴───────────────┐        ┌──────────┴────────────────────────┐
│  INFRASTRUCTURE LAYER  │        │      INFRASTRUCTURE LAYER         │
│  PostgreSQL adapters   │        │  Redis adapter, Kafka producer,   │
│  (SQLx PgPool)         │        │  S3 adapter, HTTP clients to      │
│  PostGIS spatial repos │        │  external APIs (Mapbox, Twilio)   │
└────────────────────────┘        └───────────────────────────────────┘
```

### Dependency Rule

> **Domain never imports infrastructure. Application never imports infrastructure directly. Infrastructure imports domain traits and implements them.**

In Rust terms:
- `domain` crate: no `sqlx`, no `rdkafka`, no `redis`, no `reqwest` dependencies in `Cargo.toml`.
- `application` crate: imports `domain` only. Use case structs accept `Arc<dyn ShipmentRepository>` etc.
- `infrastructure` crate: imports `domain` + `sqlx`/`rdkafka`/etc. Implements domain traits.
- `api` crate: imports `application` + `infrastructure` for wiring; sets up `AppState`.

### Directory Layout (per service)

```
services/dispatch/
├── Cargo.toml                    # workspace member
├── src/
│   ├── main.rs                   # binary entrypoint; wires AppState, starts Axum/Tonic
│   ├── domain/
│   │   ├── mod.rs
│   │   ├── entities/
│   │   │   ├── route.rs          # Route entity with domain methods
│   │   │   └── driver.rs         # Driver entity
│   │   ├── value_objects/
│   │   │   ├── geo_point.rs
│   │   │   └── route_status.rs   # enum with state machine transitions
│   │   ├── repositories/
│   │   │   ├── route_repository.rs   # pub trait RouteRepository: Send + Sync
│   │   │   └── driver_repository.rs
│   │   └── events.rs             # Domain event types
│   ├── application/
│   │   ├── mod.rs
│   │   ├── commands/
│   │   │   ├── assign_driver.rs  # AssignDriverCommand, AssignDriverResponse
│   │   │   └── optimize_route.rs
│   │   ├── queries/
│   │   │   ├── get_route.rs      # GetRouteQuery, RouteView
│   │   │   └── list_available_drivers.rs
│   │   └── services/
│   │       └── dispatch_service.rs  # pub struct DispatchService { route_repo, driver_repo, event_publisher }
│   ├── infrastructure/
│   │   ├── mod.rs
│   │   ├── postgres/
│   │   │   ├── route_repository.rs  # impl RouteRepository for PgRouteRepository
│   │   │   └── driver_repository.rs
│   │   ├── redis/
│   │   │   └── driver_location_cache.rs
│   │   └── kafka/
│   │       └── event_publisher.rs   # impl EventPublisher for KafkaEventPublisher
│   └── api/
│       ├── mod.rs
│       ├── http/
│       │   ├── routes.rs         # Axum router construction
│       │   ├── handlers/
│       │   │   ├── dispatch.rs   # POST /routes, GET /routes/:id
│       │   │   └── drivers.rs
│       │   └── middleware/
│       │       └── tenant.rs     # extracts tenant_id, sets on DB connection
│       ├── grpc/
│       │   └── dispatch_server.rs  # impl DispatchService for tonic server
│       └── mcp/
│           └── tools.rs          # MCP tool handlers → call application services
```

### AppState Wiring (main.rs pattern)

```rust
// services/dispatch/src/main.rs

#[derive(Clone)]
pub struct AppState {
    pub dispatch_service: Arc<DispatchService>,
}

pub async fn build_app_state(config: &Config) -> Result<AppState, StartupError> {
    let pg_pool = PgPoolOptions::new()
        .max_connections(config.db.max_connections)
        .connect(&config.db.url)
        .await?;

    let redis_client = redis::Client::open(config.redis.url.as_str())?;

    let route_repo: Arc<dyn RouteRepository> =
        Arc::new(PgRouteRepository::new(pg_pool.clone()));
    let driver_repo: Arc<dyn DriverRepository> =
        Arc::new(PgDriverRepository::new(pg_pool.clone()));
    let location_cache: Arc<dyn DriverLocationCache> =
        Arc::new(RedisDriverLocationCache::new(redis_client));
    let event_publisher: Arc<dyn EventPublisher> =
        Arc::new(KafkaEventPublisher::new(&config.kafka).await?);

    let dispatch_service = Arc::new(DispatchService::new(
        route_repo,
        driver_repo,
        location_cache,
        event_publisher,
    ));

    Ok(AppState { dispatch_service })
}
```

### Repository Trait Pattern (domain layer)

```rust
// services/dispatch/src/domain/repositories/route_repository.rs

use crate::domain::entities::Route;
use crate::domain::value_objects::RouteStatus;
use async_trait::async_trait;
use uuid::Uuid;

#[async_trait]
pub trait RouteRepository: Send + Sync + 'static {
    async fn find_by_id(&self, id: Uuid, tenant_id: Uuid) -> Result<Option<Route>, DomainError>;
    async fn find_active_by_driver(&self, driver_id: Uuid, tenant_id: Uuid) -> Result<Vec<Route>, DomainError>;
    async fn save(&self, route: &Route) -> Result<(), DomainError>;
    async fn update_status(&self, id: Uuid, status: RouteStatus, tenant_id: Uuid) -> Result<(), DomainError>;
}
```

### Use Case Service Pattern (application layer)

```rust
// services/dispatch/src/application/services/dispatch_service.rs

pub struct DispatchService {
    route_repo: Arc<dyn RouteRepository>,
    driver_repo: Arc<dyn DriverRepository>,
    location_cache: Arc<dyn DriverLocationCache>,
    event_publisher: Arc<dyn EventPublisher>,
}

impl DispatchService {
    pub async fn assign_driver(
        &self,
        cmd: AssignDriverCommand,
    ) -> Result<AssignDriverResponse, ApplicationError> {
        let driver = self.driver_repo
            .find_available(cmd.tenant_id, &cmd.pickup_location)
            .await?
            .ok_or(ApplicationError::NoDriverAvailable)?;

        let route = Route::create(cmd.shipment_id, driver.id, cmd.stops.clone())?;

        self.route_repo.save(&route).await?;

        self.event_publisher.publish(DriverAssignedEvent {
            route_id: route.id,
            driver_id: driver.id,
            shipment_id: cmd.shipment_id,
            tenant_id: cmd.tenant_id,
        }).await?;

        Ok(AssignDriverResponse {
            route_id: route.id,
            driver_id: driver.id,
            estimated_arrival: route.estimated_arrival(),
        })
    }
}
```

### Testing Without Infrastructure

```rust
// services/dispatch/src/application/services/dispatch_service_test.rs

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::repositories::MockRouteRepository;
    use crate::domain::repositories::MockDriverRepository;

    #[tokio::test]
    async fn assign_driver_returns_route_id_when_driver_available() {
        let mut mock_driver_repo = MockDriverRepository::new();
        mock_driver_repo
            .expect_find_available()
            .returning(|_, _| Ok(Some(Driver::fixture())));

        let mut mock_route_repo = MockRouteRepository::new();
        mock_route_repo
            .expect_save()
            .returning(|_| Ok(()));

        let service = DispatchService::new(
            Arc::new(mock_route_repo),
            Arc::new(mock_driver_repo),
            Arc::new(MockDriverLocationCache::new()),
            Arc::new(NoopEventPublisher),
        );

        let result = service.assign_driver(AssignDriverCommand::fixture()).await;
        assert!(result.is_ok());
    }
}
```

Mocks are generated via the `mockall` crate with `#[automock]` on each repository trait.

### MCP Layer Integration

MCP tool handlers in `api/mcp/tools.rs` call application services directly — the same `DispatchService` that HTTP and gRPC handlers use. No logic duplication:

```rust
pub async fn handle_assign_driver(
    params: AssignDriverParams,
    state: Arc<AppState>,
) -> Result<McpToolResult, McpError> {
    let cmd = AssignDriverCommand::from_mcp_params(params)?;
    let response = state.dispatch_service.assign_driver(cmd).await?;
    Ok(McpToolResult::json(response))
}
```

---

## Enforcement

- **Linting via `cargo-deny`**: A CI check scans `domain/Cargo.toml` and `application/Cargo.toml` for forbidden infrastructure dependencies (`sqlx`, `rdkafka`, `redis`, `reqwest`). Build fails if found.
- **Module-level `use` audit**: Clippy custom lint (via `dylint`) will flag `use sqlx::` inside `domain::` or `application::` modules.
- **Architecture fitness test**: Each service's `tests/architecture_test.rs` uses `cargo-modules` output to assert no domain→infrastructure edges exist.

---

## Consequences

### Positive

- **Domain logic testable without any running infrastructure** — mock repository implementations replace real DB calls in all unit tests. CI runs domain + application tests in under 10 seconds per service.
- **Infrastructure is swappable** — replacing PostgreSQL with a different store requires only a new struct implementing the repository trait. Domain and application code are untouched.
- **Onboarding consistency** — every service follows the same four-layer structure. A Rust engineer joining any service immediately knows where to find entities, use cases, and DB queries.
- **MCP, gRPC, and HTTP share application logic** — all three protocol entry points call the same `DispatchService`, `OrderService`, etc. No logic duplication.
- **Domain stays clean** — the domain layer is free of framework concerns. Entities are plain Rust structs; value objects enforce invariants at construction time.

### Negative

- **Higher initial scaffolding** — a new service requires creating four modules with explicit trait definitions before writing the first line of business logic. Estimated additional setup time: 1–2 hours per service. Mitigated by `scripts/dev/new-service.sh` scaffolding template.
- **More files per service** — a simple CRUD service like `carrier` has more files than strictly necessary. Accepted as the cost of uniformity.
- **`async_trait` overhead** — `#[async_trait]` macro adds a small allocation per async trait call (box'd future). Acceptable given our latency targets; can be replaced with `impl Trait` in trait position when Rust stabilizes that feature.

---

## Alternatives Considered

| Alternative | Reason Rejected |
|-------------|----------------|
| **Layered (N-tier)** | Does not enforce inward dependency rule; domain code inevitably imports infrastructure types over time |
| **CQRS + Event Sourcing** | High implementation cost for services with simple CRUD semantics; deferred to ADR-0009 for dispatch and payments |
| **Flat module structure** | Pragmatic for early prototyping; rejected because it scales poorly across 17 services and 30+ engineers |
| **Domain-Driven Design (full DDD)** | Aggregates, domain services, and bounded contexts from DDD are compatible with hexagonal architecture and will be introduced incrementally; not mandated as a whole at this stage |

---

## Related ADRs

- [ADR-0001](0001-rust-for-all-backend-services.md) — Rust for all backend microservices
- [ADR-0002](0002-event-driven-inter-service-communication.md) — Event-driven inter-service communication
- [ADR-0004](0004-mcp-for-ai-interoperability.md) — MCP for AI agent interoperability (MCP handlers call application layer)
- ADR-0009 (planned) — CQRS for dispatch and payments services
