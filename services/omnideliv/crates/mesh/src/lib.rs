//! Collaborative Agent Mesh.

pub mod events;
pub mod roles;
pub mod tools;
pub mod transition;
// runner arrives in Task 6.

pub use events::MeshEvent;
pub use transition::{MeshTransition, ProposedLine, RoutePlan, SubIntentSpec};
