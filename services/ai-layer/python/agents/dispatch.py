"""
Dispatch Agent — AI-driven driver assignment and route optimisation.

Triggered by:
  - `shipment.created` Kafka events (new orders needing assignment)
  - `driver.idle` events (available capacity to fill)
  - Manual dispatch requests via REST API

Workflow:
  1. Get shipment details
  2. Find available drivers near pickup location
  3. Check driver performance scores
  4. Assign best driver (or explain why none are suitable)
  5. Send driver confirmation notification
  6. Escalate if no suitable drivers found after 3 retries
"""
from __future__ import annotations

from typing import Any

from agents.base import BaseAgent


class DispatchAgent(BaseAgent):
    """Smart driver assignment agent with VRP-aware scoring."""

    def agent_type(self) -> str:
        return "dispatch"

    def system_prompt(self) -> str:
        return """You are the LogisticOS Dispatch Agent — an AI operator responsible for
assigning drivers to shipments as efficiently as possible.

Your goal: Assign the best available driver to a pending shipment, then notify the driver.

Decision criteria (in priority order):
1. Driver proximity to pickup location (lower distance = better)
2. Driver current workload — prefer drivers with fewer active tasks
3. Driver performance grade — prefer Excellent > Good > Fair; avoid Poor unless no choice
4. Vehicle capacity — ensure the vehicle can carry the parcel weight

Workflow:
1. Call `get_shipment` to get pickup location and parcel details
2. Call `get_available_drivers` with the pickup lat/lng
3. If no drivers found within 10km, widen to 20km radius
4. Call `get_driver_performance` for the top 3 candidates to check recent scores
5. Call `assign_driver` for the best candidate
6. Call `send_notification` to notify the driver (channel: push, template: driver_assignment)
7. If no drivers available after widening radius, call `escalate_to_human` with urgency "high"

Always state your reasoning. End your response with: "confidence: XX%"

Tenant context will be provided in each message. Always scope your actions to the correct tenant."""


class DispatchAgentFactory:
    """Creates DispatchAgent instances with tenant scoping."""

    @staticmethod
    def from_event(event: dict[str, Any]) -> DispatchAgent:
        tenant_id = event["tenant_id"]
        return DispatchAgent(tenant_id=tenant_id)

    @staticmethod
    def trigger_message(event: dict[str, Any]) -> str:
        shipment_id = event.get("shipment_id", "unknown")
        merchant_ref = event.get("merchant_reference", "")
        pickup_address = event.get("pickup_address", "")

        msg = f"New shipment requires driver assignment.\n\n"
        msg += f"Shipment ID: {shipment_id}\n"
        if merchant_ref:
            msg += f"Merchant reference: {merchant_ref}\n"
        if pickup_address:
            msg += f"Pickup address: {pickup_address}\n"
        msg += "\nPlease assign the best available driver to this shipment."
        return msg
