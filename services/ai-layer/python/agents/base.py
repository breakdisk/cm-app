"""
Base LangGraph agent — implements the react-style agentic loop using Claude.

All LogisticOS agents extend this base. The loop:
  1. State is a typed dict: {messages: list, tenant_id: str, ...agent-specific fields}
  2. `call_model` node: sends state to Claude with tools
  3. `tool_node`: executes tool calls via MCPBridge
  4. Edges: after call_model → if tool_calls → tool_node → call_model (loop)
            after call_model → if end_turn → END

Agents override `system_prompt()` and `initial_state()` to specialise behaviour.
"""
from __future__ import annotations

import uuid
from abc import ABC, abstractmethod
from typing import Any, Literal

import structlog
from langchain_anthropic import ChatAnthropic
from langchain_core.messages import AIMessage, BaseMessage, HumanMessage, SystemMessage, ToolMessage
from langgraph.graph import END, START, StateGraph
from langgraph.prebuilt import ToolNode
from pydantic import BaseModel

from config import settings
from tools.mcp_bridge import MCPBridge, build_langchain_tools

logger = structlog.get_logger(__name__)


class AgentResult(BaseModel):
    session_id: str
    agent_type: str
    tenant_id: str
    status: Literal["completed", "escalated", "failed"]
    outcome: str
    confidence: float
    actions_taken: int
    escalation_reason: str | None = None


class AgentState(dict):
    """Typed dict used as LangGraph state. Agents may add extra keys."""
    messages: list[BaseMessage]
    tenant_id: str
    session_id: str
    context: dict[str, Any]


class BaseAgent(ABC):
    """Abstract base for all LogisticOS LangGraph agents."""

    def __init__(self, tenant_id: str) -> None:
        self.tenant_id = tenant_id
        self.session_id = str(uuid.uuid4())
        self._bridge: MCPBridge | None = None
        self._graph: Any = None

    @abstractmethod
    def system_prompt(self) -> str:
        """Return the system prompt for this agent type."""

    @abstractmethod
    def agent_type(self) -> str:
        """Return agent type name for logging/telemetry."""

    def _build_graph(self, bridge: MCPBridge) -> Any:
        """Build the LangGraph StateGraph for this agent."""
        tools = build_langchain_tools(bridge)

        llm = ChatAnthropic(
            model=settings.claude_model,
            api_key=settings.anthropic_api_key,
            max_tokens=4096,
        ).bind_tools(tools)

        tool_node = ToolNode(tools)

        def call_model(state: dict[str, Any]) -> dict[str, Any]:
            messages = state["messages"]
            # Prepend system message if not already present.
            if not messages or not isinstance(messages[0], SystemMessage):
                messages = [SystemMessage(content=self.system_prompt())] + list(messages)
            response = llm.invoke(messages)
            return {"messages": list(state["messages"]) + [response]}

        def should_continue(state: dict[str, Any]) -> Literal["tools", "__end__"]:
            last = state["messages"][-1]
            if isinstance(last, AIMessage) and last.tool_calls:
                return "tools"
            return "__end__"

        graph = StateGraph(dict)
        graph.add_node("agent", call_model)
        graph.add_node("tools", tool_node)
        graph.add_edge(START, "agent")
        graph.add_conditional_edges("agent", should_continue, {"tools": "tools", "__end__": END})
        graph.add_edge("tools", "agent")
        return graph.compile()

    async def run(self, trigger_message: str, context: dict[str, Any] | None = None) -> AgentResult:
        """
        Execute the agent with a trigger message and optional context.

        Returns an AgentResult regardless of success/failure.
        """
        log = logger.bind(
            agent=self.agent_type(),
            session_id=self.session_id,
            tenant_id=self.tenant_id,
        )
        log.info("agent_started")

        async with MCPBridge(self.tenant_id, self.session_id) as bridge:
            graph = self._build_graph(bridge)

            initial_state: dict[str, Any] = {
                "messages": [HumanMessage(content=trigger_message)],
                "tenant_id": self.tenant_id,
                "session_id": self.session_id,
                "context": context or {},
            }

            actions_taken = 0
            outcome = ""
            status: Literal["completed", "escalated", "failed"] = "completed"
            escalation_reason: str | None = None

            try:
                final_state = await graph.ainvoke(
                    initial_state,
                    config={"recursion_limit": settings.agent_max_turns * 2},
                )

                messages = final_state.get("messages", [])
                actions_taken = sum(
                    1 for m in messages if isinstance(m, ToolMessage)
                )

                # Find final AI message for outcome text.
                ai_messages = [m for m in messages if isinstance(m, AIMessage)]
                if ai_messages:
                    last_ai = ai_messages[-1]
                    outcome = last_ai.content if isinstance(last_ai.content, str) else str(last_ai.content)
                    # Check if escalation was triggered.
                    if "__escalate" in outcome.lower() or "escalat" in outcome.lower():
                        status = "escalated"
                        escalation_reason = outcome[:500]

            except Exception as exc:
                log.error("agent_failed", error=str(exc))
                status = "failed"
                outcome = f"Agent failed: {exc}"

        confidence = self._estimate_confidence(outcome, status)
        log.info("agent_completed", status=status, actions=actions_taken, confidence=confidence)

        return AgentResult(
            session_id=self.session_id,
            agent_type=self.agent_type(),
            tenant_id=self.tenant_id,
            status=status,
            outcome=outcome,
            confidence=confidence,
            actions_taken=actions_taken,
            escalation_reason=escalation_reason,
        )

    def _estimate_confidence(self, outcome: str, status: str) -> float:
        """Heuristic confidence extraction from the agent's final message."""
        import re
        if status == "failed":
            return 0.0
        if status == "escalated":
            return 0.3
        match = re.search(r"confidence[:\s]+(\d+(?:\.\d+)?)\s*%", outcome, re.IGNORECASE)
        if match:
            return min(float(match.group(1)) / 100.0, 1.0)
        return 0.85
