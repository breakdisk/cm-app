"""
MCP Bridge — calls the Rust AI layer's tool execution endpoint on behalf of Python agents.

The Rust AI layer owns the canonical ToolRegistry (21 registered tools). Python agents
call tools through this bridge rather than calling downstream services directly, ensuring
a single source of truth for tool definitions and a consistent audit trail.

HTTP contract: POST /internal/tools/execute
  Request:  { "tool_name": str, "input": dict, "tenant_id": str, "session_id": str }
  Response: { "tool_use_id": str, "content": dict, "is_error": bool }
"""
from __future__ import annotations

import uuid
from typing import Any

import httpx
import structlog
from langchain_core.tools import StructuredTool
from pydantic import BaseModel, Field

from config import settings

logger = structlog.get_logger(__name__)


class ToolCallResult(BaseModel):
    tool_use_id: str
    content: dict[str, Any]
    is_error: bool


class MCPBridge:
    """HTTP client for calling Rust MCP tools from Python agents."""

    def __init__(self, tenant_id: str, session_id: str | None = None) -> None:
        self.tenant_id = tenant_id
        self.session_id = session_id or str(uuid.uuid4())
        self._client = httpx.AsyncClient(
            base_url=settings.rust_ai_layer_url,
            timeout=30.0,
        )

    async def call(self, tool_name: str, input_data: dict[str, Any]) -> ToolCallResult:
        """Execute a named tool via the Rust tool registry."""
        tool_use_id = str(uuid.uuid4())
        payload = {
            "tool_name": tool_name,
            "input": input_data,
            "tenant_id": self.tenant_id,
            "session_id": self.session_id,
            "tool_use_id": tool_use_id,
        }

        log = logger.bind(tool=tool_name, tenant_id=self.tenant_id, tool_use_id=tool_use_id)
        log.info("mcp_tool_call")

        try:
            resp = await self._client.post("/internal/tools/execute", json=payload)
            resp.raise_for_status()
            data = resp.json()
            result = ToolCallResult(**data)
        except httpx.HTTPStatusError as exc:
            log.warning("mcp_tool_http_error", status=exc.response.status_code)
            result = ToolCallResult(
                tool_use_id=tool_use_id,
                content={"error": f"HTTP {exc.response.status_code}: {exc.response.text[:200]}"},
                is_error=True,
            )
        except Exception as exc:
            log.error("mcp_tool_exception", error=str(exc))
            result = ToolCallResult(
                tool_use_id=tool_use_id,
                content={"error": str(exc)},
                is_error=True,
            )

        log.info("mcp_tool_result", is_error=result.is_error)
        return result

    async def close(self) -> None:
        await self._client.aclose()

    async def __aenter__(self) -> "MCPBridge":
        return self

    async def __aexit__(self, *_: Any) -> None:
        await self.close()


# ---------------------------------------------------------------------------
# LangChain tool wrappers — converts MCPBridge calls into StructuredTools
# that LangGraph agents can use directly in their tool node.
# ---------------------------------------------------------------------------


def _make_tool(bridge: MCPBridge, tool_name: str, description: str, schema: type[BaseModel]) -> StructuredTool:
    """Factory: wraps a single MCP tool as a LangChain StructuredTool."""

    async def _invoke(**kwargs: Any) -> str:
        result = await bridge.call(tool_name, kwargs)
        if result.is_error:
            return f"ERROR: {result.content.get('error', 'unknown error')}"
        import json
        return json.dumps(result.content, ensure_ascii=False)

    return StructuredTool.from_function(
        coroutine=_invoke,
        name=tool_name,
        description=description,
        args_schema=schema,
    )


# ── Schema models for each MCP tool ────────────────────────────────────────

class GetAvailableDriversInput(BaseModel):
    lat: float = Field(..., description="Pickup latitude")
    lng: float = Field(..., description="Pickup longitude")
    radius_km: float = Field(10.0, description="Search radius in km")


class AssignDriverInput(BaseModel):
    shipment_id: str = Field(..., description="Shipment UUID")
    driver_id: str | None = Field(None, description="Driver UUID; null for auto-assignment")


class GetShipmentInput(BaseModel):
    shipment_id: str = Field(..., description="Shipment UUID")


class RescheduleDeliveryInput(BaseModel):
    shipment_id: str = Field(..., description="Shipment UUID")
    preferred_date: str | None = Field(None, description="ISO 8601 date e.g. 2026-03-25")


class SendNotificationInput(BaseModel):
    customer_id: str = Field(..., description="Customer UUID")
    channel: str = Field(..., description="whatsapp | sms | email | push")
    template_id: str = Field(..., description="Template identifier")
    variables: dict[str, Any] = Field(default_factory=dict)


class GetDeliveryMetricsInput(BaseModel):
    from_date: str = Field(..., alias="from", description="Start date ISO 8601")
    to_date: str = Field(..., alias="to", description="End date ISO 8601")

    class Config:
        populate_by_name = True


class ReconcileCodInput(BaseModel):
    shipment_id: str = Field(..., description="Shipment UUID")


class GetDriverPerformanceInput(BaseModel):
    driver_id: str = Field(..., description="Driver UUID")
    days: int = Field(30, description="Lookback window in days")


class EscalateToHumanInput(BaseModel):
    reason: str = Field(..., description="Why human intervention is needed")
    urgency: str = Field(..., description="low | medium | high | critical")
    context: dict[str, Any] = Field(default_factory=dict)


class GetCustomerProfileInput(BaseModel):
    customer_id: str | None = Field(None, description="Customer UUID")
    phone: str | None = Field(None, description="Phone number for lookup")


class GetChurnScoreInput(BaseModel):
    customer_id: str = Field(..., description="Customer UUID")


class GetCustomerPreferencesInput(BaseModel):
    customer_id: str = Field(..., description="Customer UUID")


class GenerateInvoiceInput(BaseModel):
    shipment_id: str = Field(..., description="Shipment UUID")
    force: bool = Field(False, description="Re-generate if already exists")


class GetCodBalanceInput(BaseModel):
    merchant_id: str = Field(..., description="Merchant UUID")


class GetZoneDemandForecastInput(BaseModel):
    zone_id: str = Field(..., description="Zone identifier e.g. MM-QC-01")
    days_ahead: int = Field(7, description="Forecast horizon (max 30)")


class GetHubCapacityInput(BaseModel):
    hub_id: str = Field(..., description="Hub UUID")


class ScheduleDockInput(BaseModel):
    hub_id: str = Field(..., description="Hub UUID")
    vehicle_id: str = Field(..., description="Vehicle UUID")
    direction: str = Field(..., description="inbound | outbound")
    requested_at: str | None = Field(None, description="Preferred ISO 8601 datetime")


class GetVehicleStatusInput(BaseModel):
    vehicle_id: str = Field(..., description="Vehicle UUID")


class GetFleetAvailabilityInput(BaseModel):
    vehicle_type: str | None = Field(None, description="motorcycle | van | truck | bicycle")
    lat: float | None = Field(None, description="Reference latitude")
    lng: float | None = Field(None, description="Reference longitude")
    at_time: str | None = Field(None, description="ISO 8601 datetime")


class GetDriverLocationInput(BaseModel):
    driver_id: str = Field(..., description="Driver UUID")


class SendDriverInstructionInput(BaseModel):
    driver_id: str = Field(..., description="Driver UUID")
    instruction: str = Field(..., description="Instruction type")
    message: str = Field(..., description="Human-readable message for driver")
    payload: dict[str, Any] = Field(default_factory=dict)


def build_langchain_tools(bridge: MCPBridge) -> list[StructuredTool]:
    """Build the full set of LangChain tools backed by the MCP bridge."""
    return [
        _make_tool(bridge, "get_available_drivers",
                   "Find available drivers near a pickup location.",
                   GetAvailableDriversInput),
        _make_tool(bridge, "assign_driver",
                   "Assign a driver to a shipment.",
                   AssignDriverInput),
        _make_tool(bridge, "get_shipment",
                   "Retrieve full shipment details.",
                   GetShipmentInput),
        _make_tool(bridge, "reschedule_delivery",
                   "Reschedule a failed delivery to the next available slot.",
                   RescheduleDeliveryInput),
        _make_tool(bridge, "send_notification",
                   "Send a notification to a customer via WhatsApp, SMS, or email.",
                   SendNotificationInput),
        _make_tool(bridge, "get_delivery_metrics",
                   "Get delivery KPIs for a date range.",
                   GetDeliveryMetricsInput),
        _make_tool(bridge, "reconcile_cod",
                   "Trigger COD reconciliation for a delivered shipment.",
                   ReconcileCodInput),
        _make_tool(bridge, "get_driver_performance",
                   "Get delivery performance stats for a driver.",
                   GetDriverPerformanceInput),
        _make_tool(bridge, "escalate_to_human",
                   "Escalate to a human operator when autonomous resolution is not possible.",
                   EscalateToHumanInput),
        _make_tool(bridge, "get_customer_profile",
                   "Retrieve a unified customer profile from CDP.",
                   GetCustomerProfileInput),
        _make_tool(bridge, "get_churn_score",
                   "Get ML-predicted churn probability for a customer.",
                   GetChurnScoreInput),
        _make_tool(bridge, "get_customer_preferences",
                   "Get a customer's communication channel preferences.",
                   GetCustomerPreferencesInput),
        _make_tool(bridge, "generate_invoice",
                   "Generate or retrieve the invoice for a shipment.",
                   GenerateInvoiceInput),
        _make_tool(bridge, "get_cod_balance",
                   "Get COD wallet balance for a merchant.",
                   GetCodBalanceInput),
        _make_tool(bridge, "get_zone_demand_forecast",
                   "Get AI-predicted shipment volume forecast for a delivery zone.",
                   GetZoneDemandForecastInput),
        _make_tool(bridge, "get_hub_capacity",
                   "Get current capacity utilisation for a hub.",
                   GetHubCapacityInput),
        _make_tool(bridge, "schedule_dock",
                   "Schedule a loading dock slot at a hub for a vehicle.",
                   ScheduleDockInput),
        _make_tool(bridge, "get_vehicle_status",
                   "Get real-time status of a specific vehicle.",
                   GetVehicleStatusInput),
        _make_tool(bridge, "get_fleet_availability",
                   "Get available vehicles in the fleet.",
                   GetFleetAvailabilityInput),
        _make_tool(bridge, "get_driver_location",
                   "Get the last known GPS location of a driver.",
                   GetDriverLocationInput),
        _make_tool(bridge, "send_driver_instruction",
                   "Send an operational instruction to a driver's app.",
                   SendDriverInstructionInput),
    ]
