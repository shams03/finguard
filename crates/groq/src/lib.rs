//! Groq adapter: chat backend + two *semantic* components.
//!
//! * [`GroqIntentParser`] turns language into a structured intent.
//! * [`GroqEscalationAnalyzer`] writes a clarification when confidence is low.
//!
//! Neither can authorise or execute anything: they return data that the
//! deterministic policy layer then validates, grounds against the user's own
//! text/history, and may ignore.

pub mod backend;
pub mod escalation;
pub mod intent;
pub mod mock;

pub use backend::{
    ChatBackend, ChatRequest, ChatResponse, GroqConfig, GroqHttpBackend, ScriptedBackend,
};
pub use escalation::GroqEscalationAnalyzer;
pub use intent::GroqIntentParser;
pub use mock::{MockEscalationAnalyzer, MockLlmFailure};
