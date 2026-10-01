//! The orchestrator: language -> typed action -> hard policy -> (Jev) ->
//! (Groq escalation) -> final policy -> confirmation -> execution.
//!
//! This crate wires the pieces together but contains **no decision logic of
//! its own**: decisions come from `finguard-policy`, state changes from
//! `finguard-persistence`'s executor, and models are only ever called through
//! the `IntentParser` / `DecisionProvider` / `EscalationAnalyzer` traits.

#![allow(clippy::type_complexity)]

pub mod deterministic;
pub mod error;
pub mod pipeline;
pub mod view;

pub use deterministic::DeterministicIntentParser;
pub use error::AgentError;
pub use pipeline::{Agent, AgentConfig, AgentDeps, ClarifyAnswer, CommandOutcome};
