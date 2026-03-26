"""MCP tool bridge — wraps Rust service HTTP endpoints as LangChain/LangGraph tools."""
from tools.mcp_bridge import MCPBridge, build_langchain_tools

__all__ = ["MCPBridge", "build_langchain_tools"]
