//! Core vocabulary of FinGuard: strongly typed ids, money, actions, the action
//! state machine, risk types and the traits implemented by AI providers.
//!
//! This crate has **no I/O**. Everything here is pure and unit-testable.

pub mod action;
pub mod actor;
pub mod decision;
pub mod errors;
pub mod escalation;
pub mod ids;
pub mod intent;
pub mod money;
pub mod redact;
pub mod risk;
pub mod state;
pub mod versions;

pub use action::*;
pub use actor::*;
pub use decision::*;
pub use errors::*;
pub use escalation::*;
pub use ids::*;
pub use intent::*;
pub use money::*;
pub use redact::*;
pub use risk::*;
pub use state::*;
pub use versions::*;
