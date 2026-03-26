# ADR-0002: Kafka for Inter-Service Event Streaming

**Status:** Accepted
**Date:** 2026-03-17
**Deciders:** Principal Architect, Engineering Managers

## Context

With 17 microservices, we need a communication pattern that:
- Decouples producers from consumers (dispatch doesn't need to know about engagement)
- Handles spiky load (holiday shipping volumes, e.g. Christmas balikbayan surge)
- Enables event replay for audit, analytics, and AI model training
- Supports multiple consumers per event (delivery completed → notify customer + update CDP + trigger billing)

Alternatives: Direct gRPC calls, RabbitMQ, AWS EventBridge, Redis Streams.

## Decision

**Apache Kafka** (MSK on AWS in production, Confluent-compatible locally) for all async inter-service communication. **gRPC** (Tonic) for synchronous request-response calls where latency or consistency is critical.

## Rules

1. **State changes emit Kafka events.** Any mutation in a service that other services care about MUST produce a domain event to the canonical topic (see `libs/events/src/lib.rs` for topic registry).
2. **No cross-service DB joins.** Services never read each other's databases.
3. **Idempotent consumers.** All Kafka consumers must handle duplicate messages safely (use `event.id` for deduplication).
4. **gRPC for synchronous paths.** Auth token validation, shipment lookup during booking flow — these use gRPC for latency and strong contracts.
5. **Events follow CloudEvents spec.** Envelope: `id`, `source`, `type`, `time`, `tenant_id`, `data`.

## Consequences

- Services can be deployed and scaled independently
- AI layer subscribes to events for model training data without impacting operational services
- Engagement engine reacts to `delivery.completed` without dispatch needing to call it
- Requires operational maturity for Kafka (monitoring consumer lag, partition rebalancing)
