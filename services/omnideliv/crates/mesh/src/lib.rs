//! Collaborative Agent Mesh.

pub mod roles;
pub mod transition;
// events, runner, tools arrive in Tasks 4-6.

pub use transition::{MeshTransition, ProposedLine, RoutePlan, SubIntentSpec};
