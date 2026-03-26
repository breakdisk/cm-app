"""
Reconciliation Agent — autonomous COD reconciliation and anomaly detection.

Triggered by:
  - `delivery.completed` Kafka events where payment_method = "cod"
  - `cod.collection.anomaly` events from the fraud detection service
  - Scheduled batch run (nightly) for unreconciled COD deliveries

Workflow for standard COD reconciliation:
  1. Verify shipment is marked delivered with POD
  2. Check COD amount collected matches shipment declared amount
  3. Trigger reconciliation → credits merchant wallet
  4. If amount mismatch → escalate with urgency "high"
  5. If POD missing but delivery marked complete → escalate with urgency "medium"

Anomaly detection workflow:
  - Multiple COD collections at same address within 24h → flag for review
  - COD amount significantly higher than declared value → flag
  - Driver collecting COD but GPS not near delivery address → flag + escalate critical
"""
from __future__ import annotations

from typing import Any

from agents.base import BaseAgent


class ReconciliationAgent(BaseAgent):
    """COD reconciliation and payment anomaly detection agent."""

    def agent_type(self) -> str:
        return "reconciliation"

    def system_prompt(self) -> str:
        return """You are the LogisticOS Reconciliation Agent — an AI operator responsible for
ensuring all Cash on Delivery (COD) payments are correctly processed and credited to merchants.

Your responsibilities:
1. Verify that completed COD deliveries have been properly reconciled
2. Detect and flag suspicious COD collection patterns
3. Generate invoices for completed shipments
4. Ensure merchant wallets are credited accurately and promptly

Reconciliation rules:
- A COD delivery is eligible for reconciliation when: status = delivered AND POD = complete
- Amount tolerance: collected amount must be within PHP 5 of declared amount
- If collected > declared by more than PHP 5: flag as "overpayment" — escalate medium
- If collected < declared by more than PHP 5: flag as "underpayment" — escalate high
- If collected amount = 0 but COD marked collected: escalate critical (possible fraud)

Anomaly thresholds (escalate immediately):
- Driver GPS > 500m from delivery address at time of COD collection
- Same address receiving COD > 3 times in 24 hours
- COD amount > PHP 50,000 (high-value — requires verification)

Workflow for each shipment:
1. Get shipment details to verify status and COD amount
2. Call `reconcile_cod` to trigger reconciliation
3. Generate invoice with `generate_invoice`
4. Check if any anomalies need flagging
5. Escalate if needed

End your response with: "confidence: XX%"

All amounts are in Philippine Peso (PHP). Round to 2 decimal places."""


class ReconciliationAgentFactory:
    """Creates ReconciliationAgent instances from delivery events."""

    @staticmethod
    def from_event(event: dict[str, Any]) -> ReconciliationAgent:
        return ReconciliationAgent(tenant_id=event["tenant_id"])

    @staticmethod
    def trigger_message(event: dict[str, Any]) -> str:
        shipment_id = event.get("shipment_id", "unknown")
        cod_amount = event.get("cod_amount_php", 0)
        collected_amount = event.get("collected_amount_php", 0)
        driver_id = event.get("driver_id", "unknown")
        event_type = event.get("type", "delivery.completed")

        if event_type == "cod.collection.anomaly":
            return (
                f"COD collection anomaly detected for shipment {shipment_id}.\n"
                f"Driver ID: {driver_id}\n"
                f"Declared COD amount: PHP {cod_amount:.2f}\n"
                f"Collected amount: PHP {collected_amount:.2f}\n"
                f"Anomaly details: {event.get('anomaly_reason', 'unspecified')}\n\n"
                "Please investigate and take appropriate action."
            )

        return (
            f"COD delivery completed. Reconciliation required.\n"
            f"Shipment ID: {shipment_id}\n"
            f"Driver ID: {driver_id}\n"
            f"COD amount: PHP {cod_amount:.2f}\n\n"
            "Please reconcile the COD payment and generate the invoice."
        )
