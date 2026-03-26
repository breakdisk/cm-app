"""
Merchant Support Agent — conversational AI for merchant-facing support requests.

Handles:
  - Shipment status enquiries
  - COD balance and remittance questions
  - Billing and invoice requests
  - SLA dispute escalations
  - Carrier selection advice
  - General platform guidance

This is a multi-turn conversational agent. Unlike the Dispatch and Recovery agents
which are fire-and-forget, the support agent maintains conversation context across
multiple HTTP requests using session_id.

Triggered via:
  - REST API: POST /v1/agents/run (agent_type: merchant_support)
  - WhatsApp webhook (when merchant messages the LogisticOS WhatsApp number)
"""
from __future__ import annotations

from typing import Any

from agents.base import BaseAgent


class MerchantSupportAgent(BaseAgent):
    """Conversational support agent for merchants."""

    def agent_type(self) -> str:
        return "merchant_support"

    def system_prompt(self) -> str:
        return """You are LogisticOS Assistant — a helpful, knowledgeable support agent for
merchants using the LogisticOS last-mile delivery platform.

You help merchants with:
- Tracking shipments and understanding status updates
- COD (Cash on Delivery) balance enquiries and remittance schedules
- Invoice generation and billing questions
- Understanding delivery performance metrics
- Resolving disputes and SLA concerns
- Choosing the right carrier for their shipments

Tone: Professional but friendly. Use clear, simple language. Avoid jargon.
Format: Keep responses concise (2-4 sentences for simple queries, structured lists for complex ones).

Philippines context:
- Currency is Philippine Peso (PHP, ₱)
- COD remittances are typically processed every Tuesday and Friday
- Common pain points: NCR traffic delays, provincial address accuracy, COD disputes

When you need data to answer a question:
1. First check if you need shipment info → use `get_shipment`
2. For COD balance → use `get_cod_balance` (need merchant_id from context)
3. For performance data → use `get_delivery_metrics`
4. For invoices → use `generate_invoice`
5. If you cannot resolve the issue → use `escalate_to_human` with appropriate urgency

Never make up tracking numbers, amounts, or dates. Always retrieve live data.
If a merchant is frustrated, acknowledge their concern empathetically before looking up data.

End complex resolution responses with: "confidence: XX%"
For simple informational responses, no confidence score needed."""


class MerchantSupportAgentFactory:
    """Creates MerchantSupportAgent instances."""

    @staticmethod
    def from_request(merchant_id: str, tenant_id: str) -> MerchantSupportAgent:
        return MerchantSupportAgent(tenant_id=tenant_id)

    @staticmethod
    def trigger_message(user_message: str, context: dict[str, Any]) -> str:
        """Build the initial trigger message with available context."""
        parts = [user_message]

        if context.get("merchant_id"):
            parts.append(f"\n[Context: merchant_id={context['merchant_id']}]")
        if context.get("shipment_id"):
            parts.append(f"[Context: shipment_id={context['shipment_id']}]")
        if context.get("tracking_number"):
            parts.append(f"[Context: tracking_number={context['tracking_number']}]")

        return "\n".join(parts)
