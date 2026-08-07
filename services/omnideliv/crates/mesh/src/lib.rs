//! Collaborative Agent Mesh.

pub mod roles;
pub mod tools;
pub mod transition;
// events, runner arrive in Tasks 5-6.

pub use transition::{MeshTransition, ProposedLine, RoutePlan, SubIntentSpec};
