"""
Event → agent routing. Maps Kafka topic/event_type to the appropriate agent.

Each handler validates the event payload and dispatches to the right agent factory.
Returns an AgentResult or None if the event should be skipped (e.g. feature flag off).
"""
from __future__ import annotations

import json
from typing import Any

import structlog

from agents.dispatch import DispatchAgentFactory
from agents.recovery import RecoveryAgentFactory
from agents.reconciliation import ReconciliationAgentFactory
from agents.base import AgentResult
from config import settings

logger = structlog.get_logger(__name__)


async def route_event(topic: str, event: dict[str, Any]) -> AgentResult | None:
    """
    Route a Kafka event to the appropriate agent.

    Returns AgentResult if an agent was triggered, None if skipped.
    """
    log = logger.bind(topic=topic, tenant_id=event.get("tenant_id"), shipment_id=event.get("shipment_id"))

    if not event.get("tenant_id"):
        log.warning("event_missing_tenant_id_skipped")
        return None

    match topic:
        case "shipment.created":
            return await _handle_shipment_created(event, log)
        case "delivery.failed":
            return await _handle_delivery_failed(event, log)
        case "delivery.completed":
            return await _handle_delivery_completed(event, log)
        case "cod.collection.anomaly":
            return await _handle_cod_anomaly(event, log)
        case "driver.idle":
            # Driver idle events are informational — Dispatch agent handles proactively
            # only if there are pending unassigned shipments. Skip for now.
            log.debug("driver_idle_event_skipped")
            return None
        case "merchant.support.request":
            # Support requests are handled via REST API (POST /v1/agents/run), not Kafka.
            log.debug("support_request_via_kafka_skipped")
            return None
        case _:
            log.warning("unknown_topic_skipped", topic=topic)
            return None


async def _handle_shipment_created(event: dict[str, Any], log: Any) -> AgentResult | None:
    if not settings.dispatch_agent_enabled:
        log.info("dispatch_agent_disabled_skipped")
        return None

    shipment_id = event.get("shipment_id")
    if not shipment_id:
        log.warning("shipment_created_missing_shipment_id")
        return None

    log.info("dispatch_agent_triggered")
    agent = DispatchAgentFactory.from_event(event)
    message = DispatchAgentFactory.trigger_message(event)
    return await agent.run(message, context=event)


async def _handle_delivery_failed(event: dict[str, Any], log: Any) -> AgentResult | None:
    if not settings.recovery_agent_enabled:
        log.info("recovery_agent_disabled_skipped")
        return None

    shipment_id = event.get("shipment_id")
    if not shipment_id:
        log.warning("delivery_failed_missing_shipment_id")
        return None

    log.info("recovery_agent_triggered", attempt=event.get("attempt_number", 1))
    agent = RecoveryAgentFactory.from_event(event)
    message = RecoveryAgentFactory.trigger_message(event)
    return await agent.run(message, context=event)


async def _handle_delivery_completed(event: dict[str, Any], log: Any) -> AgentResult | None:
    if not settings.reconciliation_agent_enabled:
        log.info("reconciliation_agent_disabled_skipped")
        return None

    # Only trigger reconciliation for COD shipments.
    if event.get("payment_method") != "cod":
        log.debug("non_cod_delivery_skipped")
        return None

    shipment_id = event.get("shipment_id")
    if not shipment_id:
        log.warning("delivery_completed_missing_shipment_id")
        return None

    log.info("reconciliation_agent_triggered")
    agent = ReconciliationAgentFactory.from_event(event)
    message = ReconciliationAgentFactory.trigger_message(event)
    return await agent.run(message, context=event)


async def _handle_cod_anomaly(event: dict[str, Any], log: Any) -> AgentResult | None:
    if not settings.reconciliation_agent_enabled:
        return None

    log.info("reconciliation_agent_triggered_anomaly")
    agent = ReconciliationAgentFactory.from_event(event)
    message = ReconciliationAgentFactory.trigger_message(event)
    return await agent.run(message, context=event)
