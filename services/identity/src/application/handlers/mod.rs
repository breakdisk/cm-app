//! Command handlers — thin orchestrators that validate, call services, and return results.
//! Currently the HTTP handlers in `api::http` call application services directly.
//! This module is reserved for event-sourced command handling if we adopt CQRS fully.
