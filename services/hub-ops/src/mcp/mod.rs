//! Model Context Protocol (MCP) tools for hub-ops (ADR-0004).
//!
//! AI agents consume hub operational data and invoke actions exclusively through
//! these tools. `tenant_id` is always JWT-derived via `McpContext`, never taken
//! from tool arguments. Mounting (JSON-RPC over HTTP) is handled by the API
//! gateway; this module provides the tool registry + handlers.

pub mod audit;
pub mod context;
pub mod tools;

use std::sync::Arc;

use crate::application::services::HubTransferService;

/// Dependencies available to hub-ops MCP tool handlers.
pub struct McpState {
    pub hub_transfer_svc: Arc<HubTransferService>,
}

impl McpState {
    pub fn new(hub_transfer_svc: Arc<HubTransferService>) -> Self {
        Self { hub_transfer_svc }
    }
}
