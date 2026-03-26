"""
Recovery Agent — handles failed deliveries and chooses the optimal recovery path.

Triggered by:
  - `delivery.failed` Kafka events
  - Ops staff triggering manual recovery

Recovery strategies (in order of preference):
  1. Re-attempt same day (if time permits and customer reachable)
  2. Schedule next-day reattempt (most common)
  3. Redirect to alternative address (if customer requests)
  4. Return to sender (after 3 failed attempts or customer request)
  5. Escalate to human (complex cases: COD dispute, damaged parcel, etc.)

The agent contacts the customer first to understand their preference,
then executes the chosen recovery path.
"""
from __future__ import annotations

from typing import Any

from agents.base import BaseAgent


class RecoveryAgent(BaseAgent):
    """Autonomous failed delivery recovery agent."""

    def agent_type(self) -> str:
        return "recovery"

    def system_prompt(self) -> str:
        return """You are the LogisticOS Recovery Agent — an AI operator that handles failed
delivery attempts and determines the best recovery path.

When a delivery fails, you must:
1. Understand why it failed (no one home, wrong address, refused, damaged, etc.)
2. Retrieve the customer's profile and preferences
3. Choose the best recovery strategy
4. Execute the strategy (reschedule, notify, or escalate)

Recovery decision tree:
- Attempt count = 1 or 2: Reschedule for next available slot + notify customer via preferred channel
- Attempt count = 3: Return to sender + notify customer and merchant + generate invoice
- Failure reason = "address_not_found": Notify customer to provide correct address before rescheduling
- Failure reason = "refused_cod": Escalate with urgency "high" — possible fraud indicator
- Failure reason = "damaged": Escalate with urgency "critical" — file damage report
- Customer churn score > 0.7: Send a high-value retention notification with priority channel

Always:
- Check customer preferences before choosing notification channel
- Use WhatsApp as primary channel (highest open rate in PH market), fallback to SMS
- Include the tracking link in all customer notifications
- Log your reasoning and confidence score

End your response with: "confidence: XX%"

Philippines context: Many deliveries fail due to "no one home" (typical in NCR). Consider offering
evening delivery slots (6-9 PM) as an alternative. COD amounts are in Philippine Peso (PHP)."""

    def _recovery_trigger(self, shipment_id: str, attempt: int, reason: str, customer_id: str) -> str:
        return (
            f"Delivery failed for shipment {shipment_id}.\n"
            f"Attempt number: {attempt}\n"
            f"Failure reason: {reason}\n"
            f"Customer ID: {customer_id}\n\n"
            "Please determine the best recovery path and execute it."
        )


class RecoveryAgentFactory:
    """Creates RecoveryAgent instances from delivery failure events."""

    @staticmethod
    def from_event(event: dict[str, Any]) -> RecoveryAgent:
        return RecoveryAgent(tenant_id=event["tenant_id"])

    @staticmethod
    def trigger_message(event: dict[str, Any]) -> str:
        shipment_id = event.get("shipment_id", "unknown")
        attempt = event.get("attempt_number", 1)
        reason = event.get("failure_reason", "unknown")
        customer_id = event.get("customer_id", "unknown")

        return (
            f"Delivery failed for shipment {shipment_id}.\n"
            f"Attempt number: {attempt}\n"
            f"Failure reason: {reason}\n"
            f"Customer ID: {customer_id}\n\n"
            "Please determine the best recovery path and execute it."
        )
