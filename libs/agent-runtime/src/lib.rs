//! Product-agnostic agent runtime.
//!
//! Nothing in this crate may name a concept owned by a specific product.
//! Products supply their own roles and tools; this crate owns the loop.
//! Enforced by `scripts/check-runtime-boundary.sh` in CI.

// Modules are declared as each is written, so every task's tests can run
// before its siblings exist. All six are live by the end of Task 7.
// pub mod claude;
pub mod role;
// pub mod runner;
pub mod session;
// pub mod store;
pub mod tools;

// #[cfg(feature = "testing")]
// pub mod testing;

pub use role::AgentRole;
// pub use runner::AgentRunner;
// pub use session::{AgentAction, AgentMessage, AgentSession, MessageRole, SessionStatus};
// pub use store::SessionStore;
// pub use tools::{ToolBox, ToolContext, ToolDefinition, ToolResult};
