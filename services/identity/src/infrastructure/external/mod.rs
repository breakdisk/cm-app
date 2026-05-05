//! External service adapters for the identity service.

pub mod email;
pub use email::{EmailAdapter, SesEmailAdapter, LogEmailAdapter};
