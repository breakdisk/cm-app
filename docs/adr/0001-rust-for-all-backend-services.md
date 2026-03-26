# ADR-0001: Rust for All Backend Microservices

**Status:** Accepted
**Date:** 2026-03-17
**Deciders:** Principal Architect, CTO

## Context

LogisticOS requires a backend stack that can:
- Handle high-throughput GPS/location streams (thousands of driver updates per second)
- Execute route optimization algorithms (VRP) with sub-500ms SLA
- Process payment transactions with zero data races
- Run long-lived connections for live tracking (WebSocket / SSE)
- Operate reliably in constrained K8s pods (memory-efficient)

Candidates evaluated: Go, Node.js (TypeScript), Java (Spring Boot), Rust.

## Decision

**Rust** is the primary language for all 17 backend microservices.

## Rationale

| Concern | Rust Advantage |
|---------|---------------|
| Memory safety | Ownership system eliminates entire classes of bugs (use-after-free, data races) without GC pauses |
| Performance | Comparable to C/C++ — critical for VRP solver, real-time location processing |
| Concurrency | `async/await` + Tokio runtime handles 100k+ concurrent connections efficiently |
| Type safety | Algebraic types make invalid state unrepresentable at compile time |
| Operational cost | Lower CPU/RAM per pod → smaller K8s footprint → lower cloud bill at scale |
| Correctness | `Result<T, E>` enforced error handling — no unchecked nulls or exceptions |

## Trade-offs

- **Steeper learning curve** — mitigated by hiring Rust-experienced engineers and a shared `libs/` crate layer
- **Slower initial development** — accepted; correctness and performance are non-negotiable for payments and dispatch
- **Smaller ecosystem than Java/Go** — sufficient for our use case; key crates (Axum, SQLx, rdkafka) are production-grade

## Consequences

- All backend engineers must have or develop Rust proficiency
- AI/ML workloads (Python-native) run as sidecar services communicating via gRPC
- Shared `libs/` workspace crates reduce boilerplate across services
- `cargo build --workspace` must pass with zero warnings on all CI runs
