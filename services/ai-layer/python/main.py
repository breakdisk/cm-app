"""
LogisticOS AI Agent Sidecar — FastAPI entry point.

Runs alongside the Rust AI layer service. Responsibilities:
  1. Kafka consumer: listens for logistics events and dispatches to LangGraph agents
  2. HTTP API: accepts on-demand agent requests and exposes health/metrics endpoints
  3. Conversational support: multi-turn merchant support sessions via HTTP

Port: 8090 (Rust ai-layer runs on 8080)
"""
from __future__ import annotations

import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from prometheus_client import Counter, Histogram, make_asgi_app
from pydantic import BaseModel
from typing import Any

from agents.dispatch import DispatchAgent
from agents.recovery import RecoveryAgent
from agents.merchant_support import MerchantSupportAgent, MerchantSupportAgentFactory
from agents.reconciliation import ReconciliationAgent
from agents.base import AgentResult
from config import settings
from events.consumer import KafkaAgentConsumer

# ── Logging ────────────────────────────────────────────────────────────────
structlog.configure(
    processors=[
        structlog.contextvars.merge_contextvars,
        structlog.processors.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.dev.ConsoleRenderer() if settings.log_level == "DEBUG" else structlog.processors.JSONRenderer(),
    ],
)
logger = structlog.get_logger(__name__)

# ── Prometheus metrics ─────────────────────────────────────────────────────
agent_runs_total = Counter(
    "logisticos_agent_runs_total",
    "Total agent runs by type and status",
    ["agent_type", "status"],
)
agent_run_duration = Histogram(
    "logisticos_agent_run_duration_seconds",
    "Agent run duration",
    ["agent_type"],
)

# ── FastAPI app ────────────────────────────────────────────────────────────
app = FastAPI(
    title="LogisticOS AI Agent Sidecar",
    version="1.0.0",
    description="LangGraph agent orchestration for LogisticOS — Python sidecar",
    docs_url="/docs",
    redoc_url=None,
)

app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],  # Restricted to internal K8s network via Istio
    allow_methods=["*"],
    allow_headers=["*"],
)

# Mount Prometheus metrics at /metrics
app.mount("/metrics", make_asgi_app())

kafka_consumer = KafkaAgentConsumer()


@app.on_event("startup")
async def startup() -> None:
    logger.info("ai_sidecar_starting", port=settings.port)
    await kafka_consumer.start()
    logger.info("ai_sidecar_ready")


@app.on_event("shutdown")
async def shutdown() -> None:
    await kafka_consumer.stop()
    logger.info("ai_sidecar_stopped")


# ── Health / Ready ─────────────────────────────────────────────────────────

@app.get("/health")
async def health() -> dict[str, str]:
    return {"status": "ok", "service": "ai-agents-python"}


@app.get("/ready")
async def ready() -> dict[str, str]:
    return {"status": "ready"}


# ── Agent API ──────────────────────────────────────────────────────────────

class RunAgentRequest(BaseModel):
    agent_type: str
    prompt: str
    tenant_id: str
    context: dict[str, Any] = {}


class RunAgentResponse(BaseModel):
    session_id: str
    agent_type: str
    status: str
    outcome: str
    confidence: float
    actions_taken: int
    escalation_reason: str | None = None


@app.post("/v1/agents/run", response_model=RunAgentResponse)
async def run_agent(req: RunAgentRequest) -> RunAgentResponse:
    """
    Trigger an AI agent on-demand.

    Supported agent types: dispatch, recovery, merchant_support, reconciliation
    """
    log = logger.bind(agent_type=req.agent_type, tenant_id=req.tenant_id)
    log.info("on_demand_agent_requested")

    agent_map: dict[str, type] = {
        "dispatch": DispatchAgent,
        "recovery": RecoveryAgent,
        "merchant_support": MerchantSupportAgent,
        "reconciliation": ReconciliationAgent,
    }

    agent_cls = agent_map.get(req.agent_type)
    if not agent_cls:
        raise HTTPException(
            status_code=400,
            detail=f"Unknown agent_type '{req.agent_type}'. Valid: {list(agent_map.keys())}",
        )

    agent = agent_cls(tenant_id=req.tenant_id)

    import time
    start = time.monotonic()
    try:
        result: AgentResult = await agent.run(req.prompt, context=req.context)
    except Exception as exc:
        log.error("agent_run_exception", error=str(exc))
        raise HTTPException(status_code=500, detail=f"Agent execution failed: {exc}") from exc
    finally:
        duration = time.monotonic() - start
        agent_run_duration.labels(agent_type=req.agent_type).observe(duration)

    agent_runs_total.labels(agent_type=req.agent_type, status=result.status).inc()
    log.info("on_demand_agent_completed", status=result.status, confidence=result.confidence)

    return RunAgentResponse(
        session_id=result.session_id,
        agent_type=result.agent_type,
        status=result.status,
        outcome=result.outcome,
        confidence=result.confidence,
        actions_taken=result.actions_taken,
        escalation_reason=result.escalation_reason,
    )


# ── Support: multi-turn conversation ──────────────────────────────────────

class SupportMessageRequest(BaseModel):
    message: str
    merchant_id: str
    tenant_id: str
    context: dict[str, Any] = {}


@app.post("/v1/agents/support/message", response_model=RunAgentResponse)
async def support_message(req: SupportMessageRequest) -> RunAgentResponse:
    """
    Single-turn merchant support message.

    For multi-turn conversations, call this endpoint repeatedly with the same
    session context. The support agent maintains conversational coherence via
    the LangGraph message state.
    """
    ctx = {**req.context, "merchant_id": req.merchant_id}
    trigger = MerchantSupportAgentFactory.trigger_message(req.message, ctx)

    agent = MerchantSupportAgent(tenant_id=req.tenant_id)

    import time
    start = time.monotonic()
    result: AgentResult = await agent.run(trigger, context=ctx)
    duration = time.monotonic() - start

    agent_run_duration.labels(agent_type="merchant_support").observe(duration)
    agent_runs_total.labels(agent_type="merchant_support", status=result.status).inc()

    return RunAgentResponse(
        session_id=result.session_id,
        agent_type=result.agent_type,
        status=result.status,
        outcome=result.outcome,
        confidence=result.confidence,
        actions_taken=result.actions_taken,
        escalation_reason=result.escalation_reason,
    )


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(
        "main:app",
        host=settings.host,
        port=settings.port,
        log_level=settings.log_level.lower(),
        reload=False,
    )
