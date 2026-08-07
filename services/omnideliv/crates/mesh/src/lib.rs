//! Collaborative Agent Mesh.

pub mod conflict;
pub mod events;
pub mod roles;
pub mod runner;
pub mod tools;
pub mod transition;

pub use conflict::{Conflict, ConflictKind, ItemFacts, ReconcileContext};
pub use events::MeshEvent;
pub use runner::{MeshConfig, MeshOutcome, MeshRunner};
pub use transition::{MeshTransition, ProposedLine, RoutePlan, SubIntentSpec};
